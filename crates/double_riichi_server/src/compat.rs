use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::ws::{CloseFrame, Message, WebSocket},
    extract::{ConnectInfo, Extension, Path, State, WebSocketUpgrade, rejection::BytesRejection},
    http::{HeaderMap, StatusCode, Uri, header},
    response::Response,
};
use double_riichi_core::{
    AudienceProjection, Decision, DecisionId, GameEvent, GameMode, MatchMachine, Participant,
    ParticipantId, ParticipantKind, PlayerDecisionProjection, ROOM_COMMAND_CAPACITY, RoomCommand,
    RoomError, RoomEvent, RoomHandle, RoomRegistry, RoomResponse, Seat, TimeControl, TimingConfig,
};
use double_riichi_mjai::{
    AckStatus, ActionAck, MAX_FRAME_BYTES, MjaiAdapter, PossibleAction, ReplyDisposition,
    RequestTime, TimingBudget, TimingOutcome, encode_event, match_legal_action,
    parse_client_action, request_id_from_frame,
};
use double_riichi_replay::ReplayWriter;
use futures_util::{FutureExt, SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex as AsyncMutex, Notify, Semaphore, broadcast, mpsc, oneshot},
    task::JoinHandle,
    time::{self, Instant},
};
use url::Url;

use crate::http::{
    ConnectionPermit, RequestId, ServerState, generate_ulid, invalid_credentials, invalid_request,
    json_response, origin_not_allowed, rate_limited, room_error_response, room_not_found,
    server_busy,
};
use crate::{BotTokenRecord, BotTokenService, Storage, TokenRevoked};

const RANKED_FILL_DELAY: Duration = Duration::from_secs(5);
const OUTBOUND_CAPACITY: usize = 64;
const INPUT_CAPACITY: usize = 64;
const TURN_SECONDS: u64 = 18;
const RESPONSE_SECONDS: u64 = 18;
const WATCHDOG_SECONDS: u64 = 300;
const CLOSE_REPLACED: u16 = 4001;
const CLOSE_SLOW_CONSUMER: u16 = 4005;
const CLOSE_SESSION_EXPIRED: u16 = 4006;
const CLOSE_PROTOCOL: u16 = 1008;
const CLOSE_TOO_LARGE: u16 = 1009;
const CLOSE_BUSY: u16 = 1013;
const REVOCATION_ATTEMPT_TIMEOUT: Duration = Duration::from_millis(250);
const REVOCATION_RETRY_DELAY: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, PartialEq, Eq)]
enum CompatKind {
    Ranked,
    Validate,
}

struct CompatReplay {
    writer: Option<ReplayWriter>,
    storage: Option<Arc<Storage>>,
    replay_health: Arc<AtomicBool>,
    cleanup_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    metadata_open: bool,
    failed: bool,
    match_id: String,
}

struct QueuedBot {
    ticket: u64,
    token: BotTokenRecord,
    display_name: String,
    assignment: oneshot::Sender<Result<StartAssignment, AssignmentError>>,
    permit: Option<ConnectionPermit>,
    replay_root: Option<std::path::PathBuf>,
    replay_storage: Option<Arc<Storage>>,
}

#[derive(Clone, Copy)]
struct AssignmentError {
    code: u16,
    reason: &'static str,
}

struct StartAssignment {
    seat: Seat,
    input: mpsc::Sender<MatchInput>,
    output: mpsc::Sender<Message>,
    output_rx: mpsc::Receiver<Message>,
    control: mpsc::Receiver<CompatControl>,
    permit: ConnectionPermit,
}

#[derive(Clone, Copy)]
enum CompatControl {
    Close { code: u16, reason: &'static str },
}
enum MatchInput {
    Frame { seat: Seat, bytes: Vec<u8> },
    Disconnected { seat: Seat },
}

struct ActiveCompat {
    controls: Vec<(String, mpsc::Sender<CompatControl>)>,
    task: Option<JoinHandle<()>>,
    released: Arc<AtomicBool>,
}
struct CompatInner {
    queue: VecDeque<QueuedBot>,
    active: HashMap<u64, ActiveCompat>,
    next_ticket: u64,
    next_match: u64,
    timer_running: bool,
    timer_generation: u64,
    shutting_down: bool,
}

pub(crate) struct CompatState {
    max_active: AtomicUsize,
    max_queue: AtomicUsize,
    active_matches: AtomicUsize,
    replay_degraded: Arc<AtomicBool>,
    replay_cleanup_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    shutting_down: AtomicBool,
    shutdown_notify: Notify,
    token_service: OnceLock<Arc<BotTokenService>>,
    revocation_task: Mutex<Option<JoinHandle<()>>>,
    inner: AsyncMutex<CompatInner>,
    pub(crate) room_connections: Arc<AgentConnections>,
}

impl CompatState {
    pub(crate) fn new(max_active: usize, max_queue: usize) -> Self {
        Self {
            max_active: AtomicUsize::new(max_active),
            max_queue: AtomicUsize::new(max_queue),
            active_matches: AtomicUsize::new(0),
            replay_degraded: Arc::new(AtomicBool::new(false)),
            replay_cleanup_tasks: Arc::new(Mutex::new(Vec::new())),
            shutting_down: AtomicBool::new(false),
            shutdown_notify: Notify::new(),
            token_service: OnceLock::new(),
            revocation_task: Mutex::new(None),
            inner: AsyncMutex::new(CompatInner {
                queue: VecDeque::new(),
                active: HashMap::new(),
                next_ticket: 1,
                next_match: 1,
                timer_running: false,
                timer_generation: 0,
                shutting_down: false,
            }),
            room_connections: Arc::new(AgentConnections::default()),
        }
    }

    pub(crate) fn set_limits(&self, max_active: usize, max_queue: usize) {
        self.max_active.store(max_active, Ordering::Release);
        self.max_queue.store(max_queue, Ordering::Release);
    }

    pub(crate) fn watch_revocations(
        self: &Arc<Self>,
        mut receiver: broadcast::Receiver<TokenRevoked>,
        service: Arc<BotTokenService>,
        rooms: RoomRegistry,
    ) {
        let _ = self.token_service.set(service.clone());
        let state = Arc::clone(self);
        let task = tokio::spawn(async move {
            for token_id in service.revoked_token_ids() {
                if !deliver_revocation(&state, &rooms, &token_id).await {
                    return;
                }
            }
            loop {
                tokio::select! {
                    _ = state.shutdown_notify.notified() => return,
                    result = receiver.recv() => match result {
                        Ok(event) => {
                            if !deliver_revocation(&state, &rooms, event.token_id()).await {
                                return;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            for token_id in service.revoked_token_ids() {
                                if !deliver_revocation(&state, &rooms, &token_id).await {
                                    return;
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
            }
        });
        let previous = self
            .revocation_task
            .lock()
            .expect("revocation task lock poisoned")
            .replace(task);
        if let Some(previous) = previous {
            previous.abort();
        }
    }

    pub(crate) fn begin_shutdown(&self) {
        if !self.shutting_down.swap(true, Ordering::AcqRel) {
            self.shutdown_notify.notify_waiters();
        }
    }

    pub(crate) async fn active_count(&self) -> usize {
        self.active_matches.load(Ordering::Acquire)
    }

    pub(crate) fn replay_degraded(&self) -> bool {
        self.replay_degraded.load(Ordering::Acquire)
    }

    async fn wait_replay_cleanups(&self) {
        let tasks = std::mem::take(
            &mut *self
                .replay_cleanup_tasks
                .lock()
                .expect("replay cleanup task lock poisoned"),
        );
        for task in tasks {
            let _ = task.await;
        }
    }

    fn token_is_active(&self, token_id: &str) -> bool {
        self.token_service
            .get()
            .is_none_or(|service| service.is_active_token_id(token_id))
    }

    fn release_slot(&self, released: &AtomicBool) {
        if released
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _ =
                self.active_matches
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                        count.checked_sub(1)
                    });
        }
    }
    async fn enqueue_ranked(
        self: &Arc<Self>,
        token: BotTokenRecord,
        display_name: String,
        replay_root: Option<std::path::PathBuf>,
        replay_storage: Option<Arc<Storage>>,
        permit: ConnectionPermit,
    ) -> Result<
        (
            u64,
            oneshot::Receiver<Result<StartAssignment, AssignmentError>>,
        ),
        (),
    > {
        let (tx, rx) = oneshot::channel();
        let mut batch = None;
        let mut start_timer = None;
        let ticket;
        {
            let mut inner = self.inner.lock().await;
            if inner.shutting_down || self.shutting_down.load(Ordering::Acquire) {
                return Err(());
            }
            if inner.queue.len() >= self.max_queue.load(Ordering::Acquire) {
                return Err(());
            }
            ticket = inner.next_ticket;
            inner.next_ticket = inner.next_ticket.saturating_add(1);
            inner.queue.push_back(QueuedBot {
                ticket,
                token,
                display_name,
                assignment: tx,
                permit: Some(permit),
                replay_root,
                replay_storage,
            });
            if inner.queue.len() >= 4
                && self.active_matches.load(Ordering::Acquire)
                    < self.max_active.load(Ordering::Acquire)
            {
                cancel_ranked_timer(&mut inner);
                batch = Some(drain_batch(&mut inner));
            } else {
                start_timer = start_ranked_timer(
                    &mut inner,
                    self.active_matches.load(Ordering::Acquire),
                    self.max_active.load(Ordering::Acquire),
                );
            }
        }
        if let Some(generation) = start_timer {
            self.spawn_ranked_timer(generation);
        }
        if let Some(batch) = batch {
            self.start_batch(CompatKind::Ranked, batch).await;
            self.schedule_ranked_timer().await;
        }
        Ok((ticket, rx))
    }

    async fn enqueue_validate(
        self: &Arc<Self>,
        token: BotTokenRecord,
        display_name: String,
        permit: ConnectionPermit,
    ) -> Result<oneshot::Receiver<Result<StartAssignment, AssignmentError>>, ()> {
        let (tx, rx) = oneshot::channel();
        let ticket = {
            let mut inner = self.inner.lock().await;
            if inner.shutting_down
                || self.shutting_down.load(Ordering::Acquire)
                || self.active_matches.load(Ordering::Acquire)
                    >= self.max_active.load(Ordering::Acquire)
            {
                return Err(());
            }
            let ticket = inner.next_ticket;
            inner.next_ticket = inner.next_ticket.saturating_add(1);
            ticket
        };
        self.start_batch(
            CompatKind::Validate,
            vec![QueuedBot {
                ticket,
                token,
                display_name,
                assignment: tx,
                permit: Some(permit),
                replay_root: None,
                replay_storage: None,
            }],
        )
        .await;
        Ok(rx)
    }

    fn spawn_ranked_timer(self: &Arc<Self>, generation: u64) {
        let state = Arc::clone(self);
        tokio::spawn(async move {
            time::sleep(RANKED_FILL_DELAY).await;
            let _ = state.fill_ranked(generation).await;
        });
    }

    async fn schedule_ranked_timer(self: &Arc<Self>) {
        let generation = {
            let mut inner = self.inner.lock().await;
            start_ranked_timer(
                &mut inner,
                self.active_matches.load(Ordering::Acquire),
                self.max_active.load(Ordering::Acquire),
            )
        };
        if let Some(generation) = generation {
            self.spawn_ranked_timer(generation);
        }
    }

    async fn restart_ranked_timer(self: &Arc<Self>) {
        let generation = {
            let mut inner = self.inner.lock().await;
            cancel_ranked_timer(&mut inner);
            start_ranked_timer(
                &mut inner,
                self.active_matches.load(Ordering::Acquire),
                self.max_active.load(Ordering::Acquire),
            )
        };
        if let Some(generation) = generation {
            self.spawn_ranked_timer(generation);
        }
    }

    async fn fill_ranked(self: &Arc<Self>, generation: u64) -> bool {
        let batch = {
            let mut inner = self.inner.lock().await;
            if inner.timer_generation != generation {
                return true;
            }
            if inner.queue.is_empty() {
                inner.timer_running = false;
                None
            } else if self.shutting_down.load(Ordering::Acquire)
                || self.active_matches.load(Ordering::Acquire)
                    >= self.max_active.load(Ordering::Acquire)
            {
                inner.timer_running = false;
                inner.timer_generation = inner.timer_generation.saturating_add(1);
                None
            } else {
                inner.timer_running = false;
                Some(drain_batch(&mut inner))
            }
        };
        if let Some(batch) = batch {
            self.start_batch(CompatKind::Ranked, batch).await;
            self.schedule_ranked_timer().await;
        }
        true
    }

    async fn start_batch(self: &Arc<Self>, kind: CompatKind, batch: Vec<QueuedBot>) {
        if batch.is_empty() {
            return;
        }
        let start = {
            let mut inner = self.inner.lock().await;
            if inner.shutting_down
                || self.shutting_down.load(Ordering::Acquire)
                || self.active_matches.load(Ordering::Acquire)
                    >= self.max_active.load(Ordering::Acquire)
            {
                None
            } else {
                let id = inner.next_match;
                inner.next_match = inner.next_match.saturating_add(1);
                let released = Arc::new(AtomicBool::new(false));
                self.active_matches.fetch_add(1, Ordering::AcqRel);
                inner.active.insert(
                    id,
                    ActiveCompat {
                        controls: Vec::new(),
                        task: None,
                        released: Arc::clone(&released),
                    },
                );
                Some((id, released))
            }
        };
        let Some((match_id, released)) = start else {
            for bot in batch {
                let _ = bot.assignment.send(Err(AssignmentError {
                    code: CLOSE_BUSY,
                    reason: "server_busy",
                }));
            }
            return;
        };
        let prepared = prepare_match(
            match_id,
            kind,
            batch,
            Arc::clone(&self.replay_degraded),
            Arc::clone(&self.replay_cleanup_tasks),
        );
        let (mut actor, assignments, controls) = match prepared {
            Ok(value) => value,
            Err((_error, bots)) => {
                self.finish(match_id).await;
                for bot in bots {
                    let _ = bot.assignment.send(Err(AssignmentError {
                        code: CLOSE_PROTOCOL,
                        reason: "protocol_error",
                    }));
                }
                return;
            }
        };
        let players = actor.machine.players().to_vec();
        if let Some(replay) = actor.replay.as_mut() {
            replay.open(actor.mode, &players).await;
        }
        let revoked = {
            let mut inner = self.inner.lock().await;
            if !actor
                .bots
                .iter()
                .all(|bot| self.token_is_active(bot.token.token_id()))
            {
                inner.active.remove(&match_id);
                true
            } else {
                if let Some(active) = inner.active.get_mut(&match_id) {
                    active.controls = controls.clone();
                }
                false
            }
        };
        if revoked {
            self.release_slot(&released);
            for bot in actor.bots.drain(..) {
                let _ = bot.assignment.send(Err(AssignmentError {
                    code: CLOSE_SESSION_EXPIRED,
                    reason: "token_revoked",
                }));
            }
            self.restart_ranked_timer().await;
            return;
        }
        for (bot, assignment) in actor.bots.drain(..).zip(assignments) {
            let _ = bot.assignment.send(Ok(assignment));
        }
        let supervisor_controls = controls;
        let lease = ActiveMatchLease {
            state: Arc::clone(self),
            match_id,
            released,
        };
        let task = tokio::spawn(async move {
            let _lease = lease;
            let panicked = std::panic::AssertUnwindSafe(actor.run())
                .catch_unwind()
                .await
                .is_err();
            if panicked {
                for (_, sender) in supervisor_controls {
                    let _ = sender.try_send(CompatControl::Close {
                        code: CLOSE_PROTOCOL,
                        reason: "protocol_error",
                    });
                }
            }
        });
        let mut task = Some(task);
        let abort = {
            let mut inner = self.inner.lock().await;
            if inner.shutting_down {
                true
            } else if let Some(active) = inner.active.get_mut(&match_id) {
                active.task = task.take();
                false
            } else {
                true
            }
        };
        if abort && let Some(task) = task {
            task.abort();
        }
    }

    async fn finish(self: &Arc<Self>, match_id: u64) {
        let active = self.inner.lock().await.active.remove(&match_id);
        if let Some(active) = active {
            self.release_slot(&active.released);
        }
        self.restart_ranked_timer().await;
    }

    async fn cancel_waiting(self: &Arc<Self>, ticket: u64) {
        let mut inner = self.inner.lock().await;
        if let Some(index) = inner.queue.iter().position(|bot| bot.ticket == ticket) {
            inner.queue.remove(index);
        }
        if inner.queue.is_empty() {
            cancel_ranked_timer(&mut inner);
        }
        drop(inner);
        self.schedule_ranked_timer().await;
    }

    async fn revoke_token_locked(self: &Arc<Self>, token_id: &str) -> bool {
        let (waiting, mut controls) = {
            let mut inner = self.inner.lock().await;
            let mut waiting = Vec::new();
            let mut retained = VecDeque::with_capacity(inner.queue.len());
            while let Some(bot) = inner.queue.pop_front() {
                if bot.token.token_id() == token_id {
                    waiting.push(bot.assignment);
                } else {
                    retained.push_back(bot);
                }
            }
            inner.queue = retained;
            if inner.queue.is_empty() {
                cancel_ranked_timer(&mut inner);
            }
            let controls = inner
                .active
                .values()
                .flat_map(|active| {
                    active
                        .controls
                        .iter()
                        .filter(|(id, _)| id == token_id)
                        .map(|(_, sender)| sender.clone())
                })
                .collect::<Vec<_>>();
            (waiting, controls)
        };
        controls.extend(self.room_connections.controls_for_token(token_id));
        for assignment in waiting {
            let _ = assignment.send(Err(AssignmentError {
                code: CLOSE_SESSION_EXPIRED,
                reason: "token_revoked",
            }));
        }
        for sender in controls {
            let delivered = time::timeout(
                REVOCATION_ATTEMPT_TIMEOUT,
                sender.send(CompatControl::Close {
                    code: CLOSE_SESSION_EXPIRED,
                    reason: "token_revoked",
                }),
            )
            .await;
            if delivered.is_err() {
                return false;
            }
        }
        self.schedule_ranked_timer().await;
        true
    }

    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    pub(crate) async fn shutdown(&self) {
        self.begin_shutdown();
        let (waiting, mut controls, tasks) = {
            let mut inner = self.inner.lock().await;
            inner.shutting_down = true;
            inner.timer_running = false;
            inner.timer_generation = inner.timer_generation.saturating_add(1);
            let waiting = inner
                .queue
                .drain(..)
                .map(|bot| bot.assignment)
                .collect::<Vec<_>>();
            let active = std::mem::take(&mut inner.active);
            let mut controls = Vec::new();
            let mut tasks = Vec::new();
            for (_, mut active) in active {
                active.released.store(true, Ordering::Release);
                controls.extend(active.controls.into_iter().map(|(_, sender)| sender));
                if let Some(task) = active.task.take() {
                    tasks.push(task);
                }
            }
            self.active_matches.store(0, Ordering::Release);
            (waiting, controls, tasks)
        };
        controls.extend(self.room_connections.all_controls());
        for assignment in waiting {
            let _ = assignment.send(Err(AssignmentError {
                code: CLOSE_SESSION_EXPIRED,
                reason: "server_shutdown",
            }));
        }
        for sender in controls {
            let _ = time::timeout(
                REVOCATION_ATTEMPT_TIMEOUT,
                sender.send(CompatControl::Close {
                    code: CLOSE_SESSION_EXPIRED,
                    reason: "server_shutdown",
                }),
            )
            .await;
        }
        for mut task in tasks {
            task.abort();
            let _ = time::timeout(REVOCATION_ATTEMPT_TIMEOUT, &mut task).await;
        }
        let revocation_task = self
            .revocation_task
            .lock()
            .expect("revocation task lock poisoned")
            .take();
        if let Some(mut task) = revocation_task
            && time::timeout(REVOCATION_ATTEMPT_TIMEOUT, &mut task)
                .await
                .is_err()
        {
            task.abort();
        }
        self.wait_replay_cleanups().await;
    }
}

async fn deliver_revocation(
    state: &Arc<CompatState>,
    rooms: &RoomRegistry,
    token_id: &str,
) -> bool {
    loop {
        if state.is_shutting_down() {
            return false;
        }
        let transition = tokio::select! {
            _ = state.shutdown_notify.notified() => return false,
            permit = state.room_connections.transition.clone().acquire_owned() => {
                permit.expect("agent transition semaphore closed")
            }
        };
        let compat_delivered = state.revoke_token_locked(token_id).await;
        let rooms_delivered = matches!(
            time::timeout(REVOCATION_ATTEMPT_TIMEOUT, rooms.revoke_token(token_id)).await,
            Ok(Ok(()))
        );
        drop(transition);
        if compat_delivered && rooms_delivered {
            return true;
        }
        tokio::select! {
            _ = state.shutdown_notify.notified() => return false,
            _ = time::sleep(REVOCATION_RETRY_DELAY) => {}
        }
    }
}

struct ActiveMatchLease {
    state: Arc<CompatState>,
    match_id: u64,
    released: Arc<AtomicBool>,
}

impl Drop for ActiveMatchLease {
    fn drop(&mut self) {
        self.state.release_slot(&self.released);
        let state = Arc::clone(&self.state);
        let match_id = self.match_id;
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let _ = state.inner.lock().await.active.remove(&match_id);
                state.restart_ranked_timer().await;
            });
        }
    }
}

fn start_ranked_timer(
    inner: &mut CompatInner,
    active_matches: usize,
    max_active: usize,
) -> Option<u64> {
    if inner.shutting_down
        || inner.queue.is_empty()
        || active_matches >= max_active
        || inner.timer_running
    {
        return None;
    }
    inner.timer_running = true;
    inner.timer_generation = inner.timer_generation.saturating_add(1);
    Some(inner.timer_generation)
}

fn cancel_ranked_timer(inner: &mut CompatInner) {
    if inner.timer_running {
        inner.timer_running = false;
        inner.timer_generation = inner.timer_generation.saturating_add(1);
    }
}

fn drain_batch(inner: &mut CompatInner) -> Vec<QueuedBot> {
    (0..4).filter_map(|_| inner.queue.pop_front()).collect()
}

pub(crate) struct AgentConnections {
    next_generation: AtomicU64,
    entries: Mutex<HashMap<ParticipantId, AgentConnection>>,
    transition: Arc<Semaphore>,
}
struct AgentConnection {
    generation: u64,
    token_id: String,
    control: mpsc::Sender<CompatControl>,
}

impl Default for AgentConnections {
    fn default() -> Self {
        Self {
            next_generation: AtomicU64::new(0),
            entries: Mutex::new(HashMap::new()),
            transition: Arc::new(Semaphore::new(1)),
        }
    }
}

impl AgentConnections {
    fn register(
        &self,
        id: ParticipantId,
        token_id: String,
    ) -> (u64, mpsc::Receiver<CompatControl>) {
        let (tx, rx) = mpsc::channel(2);
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
        let old = self
            .entries
            .lock()
            .expect("agent connection lock poisoned")
            .insert(
                id,
                AgentConnection {
                    generation,
                    token_id,
                    control: tx,
                },
            );
        if let Some(old) = old {
            let _ = old.control.try_send(CompatControl::Close {
                code: CLOSE_REPLACED,
                reason: "connected_elsewhere",
            });
        }
        (generation, rx)
    }
    fn is_current(&self, id: &ParticipantId, generation: u64) -> bool {
        self.entries
            .lock()
            .expect("agent connection lock poisoned")
            .get(id)
            .is_some_and(|entry| entry.generation == generation)
    }
    fn remove(&self, id: &ParticipantId, generation: u64) {
        let mut entries = self.entries.lock().expect("agent connection lock poisoned");
        if entries
            .get(id)
            .is_some_and(|entry| entry.generation == generation)
        {
            entries.remove(id);
        }
    }

    fn controls_for_token(&self, token_id: &str) -> Vec<mpsc::Sender<CompatControl>> {
        self.entries
            .lock()
            .expect("agent connection lock poisoned")
            .values()
            .filter(|entry| entry.token_id == token_id)
            .map(|entry| entry.control.clone())
            .collect()
    }

    fn all_controls(&self) -> Vec<mpsc::Sender<CompatControl>> {
        self.entries
            .lock()
            .expect("agent connection lock poisoned")
            .values()
            .map(|entry| entry.control.clone())
            .collect()
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AgentJoinRequest {
    display_name: String,
}

pub(crate) async fn status(State(state): State<Arc<ServerState>>) -> Response {
    let active_matches = state.compat.active_count().await;
    json_response(
        StatusCode::OK,
        json!({"status":"ok", "active_matches": active_matches}),
    )
}

pub(crate) async fn ranked_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    uri: Uri,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if !origin_allowed(&headers, &state) {
        return origin_not_allowed(&request_id);
    }
    let token = match state.authenticate_bot(&headers, peer.map(|p| p.0.0)) {
        Ok(token) => token,
        Err(_) => return invalid_credentials(&request_id),
    };
    let display_name = query_param(uri.query(), "display_name")
        .as_deref()
        .and_then(normalize_name)
        .unwrap_or_else(|| token.name().to_owned());
    let Some(permit) = state.connection_permit() else {
        return server_busy(&request_id);
    };
    let compat = Arc::clone(&state.compat);
    let replay_root = state.replay_root();
    let replay_storage = state.replay_storage();
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            match compat
                .enqueue_ranked(token, display_name, replay_root, replay_storage, permit)
                .await
            {
                Ok((ticket, assignment)) => {
                    run_waiting_socket(socket, compat, Some(ticket), assignment).await
                }
                Err(_) => {
                    let mut socket = socket;
                    let _ = socket.send(close_message(CLOSE_BUSY, "server_busy")).await;
                }
            }
        })
}

pub(crate) async fn validate_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if !origin_allowed(&headers, &state) {
        return origin_not_allowed(&request_id);
    }
    let token = match state.authenticate_bot(&headers, peer.map(|p| p.0.0)) {
        Ok(token) => token,
        Err(_) => return invalid_credentials(&request_id),
    };
    let Some(permit) = state.connection_permit() else {
        return server_busy(&request_id);
    };
    let compat = Arc::clone(&state.compat);
    let display_name = token.name().to_owned();
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            match compat.enqueue_validate(token, display_name, permit).await {
                Ok(assignment) => run_waiting_socket(socket, compat, None, assignment).await,
                Err(_) => {
                    let mut socket = socket;
                    let _ = socket.send(close_message(CLOSE_BUSY, "server_busy")).await;
                }
            }
        })
}

async fn run_waiting_socket(
    mut socket: WebSocket,
    compat: Arc<CompatState>,
    ticket: Option<u64>,
    mut assignment: oneshot::Receiver<Result<StartAssignment, AssignmentError>>,
) {
    loop {
        tokio::select! {
            result = &mut assignment => {
                match result {
                    Ok(Ok(assignment)) => run_assigned_socket(socket, assignment).await,
                    Ok(Err(error)) => { let _ = socket.send(close_message(error.code, error.reason)).await; }
                    Err(_) => { let _ = socket.send(close_message(CLOSE_SESSION_EXPIRED, "session_expired")).await; }
                }
                return;
            }
            message = socket.next() => match message {
                Some(Ok(Message::Ping(payload))) => { let _ = socket.send(Message::Pong(payload)).await; }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => { if let Some(ticket) = ticket { compat.cancel_waiting(ticket).await; } return; }
                Some(Ok(_)) => {}
            }
        }
    }
}

async fn run_assigned_socket(socket: WebSocket, assignment: StartAssignment) {
    let StartAssignment {
        seat,
        input,
        output,
        mut output_rx,
        mut control,
        permit,
    } = assignment;
    let (mut sender, mut receiver) = socket.split();
    let mut writer = tokio::spawn(async move {
        while let Some(message) = output_rx.recv().await {
            let close = matches!(message, Message::Close(_));
            if !matches!(
                time::timeout(Duration::from_secs(1), sender.send(message)).await,
                Ok(Ok(()))
            ) || close
            {
                break;
            }
        }
    });
    let mut writer_finished = false;
    let mut close_queued = false;
    loop {
        tokio::select! {
            _ = &mut writer => { writer_finished = true; break; },
            command = control.recv() => {
                if let Some(CompatControl::Close { code, reason }) = command { let _ = queue_output(&output, close_message(code, reason)); close_queued = true; }
                break;
            }
            message = receiver.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if text.len() > MAX_FRAME_BYTES { let _ = queue_output(&output, close_message(CLOSE_TOO_LARGE, "message_too_large")); close_queued = true; break; }
                    if input.try_send(MatchInput::Frame { seat, bytes: text.as_bytes().to_vec() }).is_err() { let _ = queue_output(&output, close_message(CLOSE_SLOW_CONSUMER, "slow_consumer")); close_queued = true; break; }
                }
                Some(Ok(Message::Ping(payload))) => { let _ = queue_output(&output, Message::Pong(payload)); }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            }
        }
    }
    let _ = input.try_send(MatchInput::Disconnected { seat });
    drop(output);
    if !writer_finished {
        if close_queued {
            let _ = time::timeout(Duration::from_secs(1), &mut writer).await;
        } else {
            writer.abort();
        }
    }
    drop(permit);
}

struct CompatMatch {
    kind: CompatKind,
    mode: GameMode,
    machine: MatchMachine,
    input_rx: mpsc::Receiver<MatchInput>,
    players: HashMap<Seat, PlayerRuntime>,
    bots: Vec<QueuedBot>,
    validation_failed: bool,
    validation_reason: Option<&'static str>,
    replay: Option<CompatReplay>,
    event_cursor: usize,
}
struct PlayerRuntime {
    output: mpsc::Sender<Message>,
    adapter: MjaiAdapter,
    active: bool,
}

#[derive(Clone, Copy)]
struct PendingRequest {
    opened_at: Instant,
    deadline: Instant,
}

impl PendingRequest {
    fn elapsed_ms_at(self, now: Instant) -> u64 {
        now.saturating_duration_since(self.opened_at).as_millis() as u64
    }
}

fn due_requests(pending: &HashMap<Seat, PendingRequest>, now: Instant) -> Vec<(Seat, u64)> {
    let mut due = pending
        .iter()
        .filter(|(_, request)| request.deadline <= now)
        .map(|(seat, request)| (*seat, request.elapsed_ms_at(now)))
        .collect::<Vec<_>>();
    due.sort_by_key(|(seat, _)| *seat);
    due
}

impl CompatReplay {
    fn mark_degraded(&self) {
        self.replay_health.store(true, Ordering::Release);
    }

    async fn open(&mut self, mode: GameMode, players: &[Participant]) {
        let Some(storage) = self.storage.as_ref() else {
            return;
        };
        let Some(writer) = self.writer.as_ref() else {
            return;
        };
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if let Err(error) = storage
            .open_ranked_match(
                &self.match_id,
                mode,
                started_at,
                &writer.relative_path_string(),
                players,
            )
            .await
        {
            self.failed = true;
            self.mark_degraded();
            report_replay_failure(&self.match_id, "open", &error);
            if let Some(writer) = self.writer.take() {
                writer.abort();
            }
        } else {
            self.metadata_open = true;
        }
    }

    fn record(&mut self, events: &[GameEvent]) {
        if self.failed {
            return;
        }
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        for event in events {
            if let Err(error) = writer.append(event.clone()) {
                self.failed = true;
                self.mark_degraded();
                report_replay_failure(&self.match_id, "append", &error);
                break;
            }
        }
        if self.failed {
            self.writer.take();
        }
    }

    async fn finish(mut self, result: Option<&double_riichi_core::MatchResult>) {
        if self.failed {
            self.delete_metadata().await;
            return;
        }
        let Some(writer) = self.writer.take() else {
            self.delete_metadata().await;
            return;
        };
        let artifact = match writer.finalize() {
            Ok(artifact) => artifact,
            Err(error) => {
                self.mark_degraded();
                report_replay_failure(&self.match_id, "finalize", &error);
                self.delete_metadata().await;
                return;
            }
        };
        let Some(result) = result else {
            self.mark_degraded();
            report_replay_failure(&self.match_id, "result", &"completed match had no result");
            if let Some(storage) = &self.storage {
                let _ = storage.remove_replay_file(&artifact.relative_path_string());
            }
            self.delete_metadata().await;
            return;
        };
        if let Some(storage) = &self.storage {
            match storage
                .complete_ranked_match(
                    &self.match_id,
                    &artifact,
                    result,
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                )
                .await
            {
                Ok(()) => self.metadata_open = false,
                Err(error) => {
                    self.mark_degraded();
                    report_replay_failure(&self.match_id, "metadata", &error);
                    let _ = storage.remove_replay_file(&artifact.relative_path_string());
                    self.delete_metadata().await;
                }
            }
        }
    }

    async fn abort(self) {
        let mut this = self;
        this.writer.take();
        this.delete_metadata().await;
    }

    async fn delete_metadata(&mut self) {
        if !self.metadata_open {
            return;
        }
        let Some(storage) = &self.storage else {
            self.metadata_open = false;
            return;
        };
        match storage.delete_writing_match(&self.match_id).await {
            Ok(()) => self.metadata_open = false,
            Err(error) => {
                self.mark_degraded();
                report_replay_failure(&self.match_id, "cleanup", &error);
            }
        }
    }
}

impl Drop for CompatReplay {
    fn drop(&mut self) {
        if !self.metadata_open {
            return;
        }
        self.metadata_open = false;
        let Some(storage) = self.storage.clone() else {
            return;
        };
        let match_id = self.match_id.clone();
        let replay_health = Arc::clone(&self.replay_health);
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            replay_health.store(true, Ordering::Release);
            return;
        };
        let task = handle.spawn(async move {
            if let Err(error) = storage.delete_writing_match(&match_id).await {
                replay_health.store(true, Ordering::Release);
                report_replay_failure(&match_id, "cleanup", &error);
            }
        });
        self.cleanup_tasks
            .lock()
            .expect("replay cleanup task lock poisoned")
            .push(task);
    }
}

fn report_replay_failure(match_id: &str, stage: &str, error: &dyn std::fmt::Display) {
    tracing::warn!(match_id, stage, error = %error, "ranked replay persistence degraded");
}

fn prepare_match(
    match_id: u64,
    kind: CompatKind,
    mut bots: Vec<QueuedBot>,
    replay_health: Arc<AtomicBool>,
    cleanup_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> Result<
    (
        CompatMatch,
        Vec<StartAssignment>,
        Vec<(String, mpsc::Sender<CompatControl>)>,
    ),
    ((), Vec<QueuedBot>),
> {
    let mode = match kind {
        CompatKind::Ranked => GameMode::FourPlayerRedHalf,
        CompatKind::Validate => GameMode::FourPlayerRedEast,
    };
    let mut participants = bots
        .iter()
        .map(|bot| {
            Participant::new(
                ParticipantId::new(format!("compat-{}", bot.ticket)),
                bot.display_name.clone(),
                ParticipantKind::MJAI,
            )
        })
        .collect::<Vec<_>>();
    for index in participants.len()..mode.seat_count() {
        participants.push(Participant::new(
            ParticipantId::new(format!("compat-builtin-{match_id}-{index}")),
            format!("Tsumogiri Bot {}", index + 1),
            ParticipantKind::BuiltInBot,
        ));
    }
    let mut machine = None;
    if kind == CompatKind::Validate {
        let external = participants[0].id.clone();
        for seed in 0..512 {
            if let Ok(candidate) = MatchMachine::with_seed(mode, participants.clone(), seed)
                && candidate
                    .players()
                    .first()
                    .is_some_and(|player| player.id == external)
            {
                machine = Some(candidate);
                break;
            }
        }
    }
    let mut machine = match machine {
        Some(machine) => machine,
        None => match MatchMachine::with_seed(mode, participants, match_id) {
            Ok(machine) => machine,
            Err(_) => return Err(((), bots)),
        },
    };
    machine.set_time_control(TimeControl::RiichiDev);
    let Ok(timing) = TimingConfig::new(TURN_SECONDS, RESPONSE_SECONDS, WATCHDOG_SECONDS) else {
        return Err(((), bots));
    };
    machine.set_timing(timing);
    let (input, input_rx) = mpsc::channel(INPUT_CAPACITY);
    let mut players = HashMap::new();
    let mut assignments = Vec::new();
    let mut controls = Vec::new();
    for bot in &mut bots {
        let participant_id = ParticipantId::new(format!("compat-{}", bot.ticket));
        let Some(index) = machine
            .players()
            .iter()
            .position(|player| player.id == participant_id)
        else {
            return Err(((), bots));
        };
        let Some(seat) = Seat::new(index as u8) else {
            return Err(((), bots));
        };
        let (output, output_rx) = mpsc::channel(OUTBOUND_CAPACITY + 1);
        let (control_tx, control_rx) = mpsc::channel(2);
        controls.push((bot.token.token_id().to_owned(), control_tx));
        players.insert(
            seat,
            PlayerRuntime {
                output: output.clone(),
                adapter: MjaiAdapter::new(mode),
                active: true,
            },
        );
        let Some(permit) = bot.permit.take() else {
            return Err(((), bots));
        };
        assignments.push(StartAssignment {
            seat,
            input: input.clone(),
            output,
            output_rx,
            control: control_rx,
            permit,
        });
    }
    let replay_root = bots.first().and_then(|bot| bot.replay_root.clone());
    let replay_storage = bots.first().and_then(|bot| bot.replay_storage.clone());
    let replay_id = format!("ranked-{match_id}");
    let replay =
        replay_root.and_then(
            |root| match ReplayWriter::new(&root, replay_id.clone(), mode) {
                Ok(writer) => Some(CompatReplay {
                    writer: Some(writer),
                    storage: replay_storage,
                    replay_health: Arc::clone(&replay_health),
                    cleanup_tasks: Arc::clone(&cleanup_tasks),
                    metadata_open: false,
                    failed: false,
                    match_id: replay_id.clone(),
                }),
                Err(error) => {
                    replay_health.store(true, Ordering::Release);
                    report_replay_failure(&replay_id, "create", &error);
                    None
                }
            },
        );
    Ok((
        CompatMatch {
            kind,
            mode,
            machine,
            input_rx,
            players,
            bots,
            validation_failed: false,
            validation_reason: None,
            replay,
            event_cursor: 0,
        },
        assignments,
        controls,
    ))
}

fn close_message(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}

fn queue_output(output: &mpsc::Sender<Message>, message: Message) -> bool {
    if !matches!(message, Message::Close(_)) && output.capacity() <= 1 {
        let _ = output.try_send(close_message(CLOSE_SLOW_CONSUMER, "slow_consumer"));
        return false;
    }
    output.try_send(message).is_ok()
}

fn normalize_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 64 || value.chars().any(char::is_control) {
        None
    } else {
        Some(value.to_owned())
    }
}
fn query_param(query: Option<&str>, key: &str) -> Option<String> {
    query?.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name == key).then(|| value.replace('+', " "))
    })
}
fn origin_allowed(headers: &HeaderMap, state: &ServerState) -> bool {
    let Some(value) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(origin) = value.to_str().ok().and_then(|value| Url::parse(value).ok()) else {
        return false;
    };
    let public = state.public_origin_url();
    origin.scheme() == public.scheme()
        && origin.host_str() == public.host_str()
        && origin.port_or_known_default() == public.port_or_known_default()
        && origin.username().is_empty()
        && origin.password().is_none()
}

impl CompatMatch {
    async fn run(&mut self) {
        let initial_len = self.machine.events().len();
        let mut event_cursor = initial_len;
        let initial = self.machine.events().to_vec();
        if !self.broadcast(&initial).await {
            self.close(CLOSE_PROTOCOL, "protocol_error");
            self.abort_replay().await;
            return;
        }
        let mut recent = Vec::new();
        for _ in 0..100_000 {
            if self.machine.is_complete() {
                break;
            }
            match self.machine.resolve_expired() {
                Ok(Some(result)) => {
                    recent = result.events().to_vec();
                    self.broadcast(&recent).await;
                    event_cursor = self.machine.events().len();
                    continue;
                }
                Ok(None) => {}
                Err(_) => {
                    self.close(CLOSE_PROTOCOL, "protocol_error");
                    self.abort_replay().await;
                    return;
                }
            }
            let Ok(Some(decision)) = self.machine.current_decision() else {
                break;
            };
            let now = Instant::now();
            let mut resolved = None;
            for seat in decision
                .eligible()
                .filter(|seat| self.players.get(seat).is_none_or(|player| !player.active))
                .filter(|seat| {
                    decision.submitted_action_id(*seat).is_none()
                        && !decision.is_expired_for(*seat, now)
                })
                .collect::<Vec<_>>()
            {
                let action = decision.default_action_id(seat).clone();
                match self
                    .machine
                    .submit_action(seat, decision.id().clone(), action)
                {
                    Ok(result) if result.is_resolved() => {
                        resolved = Some(result);
                        break;
                    }
                    Ok(_) => {}
                    Err(_) => {
                        self.close(CLOSE_PROTOCOL, "protocol_error");
                        self.abort_replay().await;
                        return;
                    }
                }
            }
            if let Some(result) = resolved {
                recent = result.events().to_vec();
                self.broadcast(&recent).await;
                event_cursor = self.machine.events().len();
                continue;
            }
            let decision = match self.machine.current_decision() {
                Ok(Some(decision)) => decision,
                _ => break,
            };
            let active = decision
                .eligible()
                .filter(|seat| self.players.get(seat).is_some_and(|player| player.active))
                .collect::<Vec<_>>();
            if active.is_empty() {
                continue;
            }
            let mut pending = HashMap::new();
            for seat in active {
                let request = {
                    let Some(player) = self.players.get_mut(&seat) else {
                        continue;
                    };
                    match player
                        .adapter
                        .open_request_for_machine(&mut self.machine, seat, &recent)
                    {
                        Ok(request) => request,
                        Err(_) => {
                            player.active = false;
                            continue;
                        }
                    }
                };
                let Ok(text) = serde_json::to_string(&request) else {
                    if let Some(player) = self.players.get_mut(&seat) {
                        player.active = false;
                    }
                    continue;
                };
                let opened_at = Instant::now();
                if !self.send(seat, Message::text(text)) {
                    if let Some(player) = self.players.get_mut(&seat) {
                        player.active = false;
                    }
                    continue;
                }
                pending.insert(
                    seat,
                    PendingRequest {
                        opened_at,
                        deadline: opened_at + Duration::from_millis(request.time.deadline_ms),
                    },
                );
            }
            recent.clear();
            self.collect(&mut pending).await;
            let events = self.machine.events();
            let from = event_cursor.min(events.len());
            recent = events[from..].to_vec();
            event_cursor = events.len();
            self.broadcast(&recent).await;
        }
        if !self.machine.is_complete() {
            self.close(CLOSE_PROTOCOL, "protocol_error");
            self.abort_replay().await;
            return;
        }
        let events = self.machine.events();
        let from = self.event_cursor.min(events.len());
        if from < events.len() {
            let pending = events[from..].to_vec();
            self.broadcast(&pending).await;
        }
        if self.kind == CompatKind::Validate {
            let mut result = json!({"type":"validation_result", "passed": !self.validation_failed});
            if let Some(reason) = self.validation_reason {
                result["reason"] = Value::String(reason.to_owned());
            }
            for seat in self.players.keys().copied().collect::<Vec<_>>() {
                let _ = self.send(seat, Message::text(result.to_string()));
            }
        }
        let result = self.machine.result().cloned();
        if let Some(replay) = self.replay.take() {
            replay.finish(result.as_ref()).await;
        }
        self.close(1000, "complete");
    }

    fn record_replay(&mut self, events: &[GameEvent]) {
        if let Some(replay) = self.replay.as_mut() {
            replay.record(events);
        }
    }

    async fn abort_replay(&mut self) {
        if let Some(replay) = self.replay.take() {
            replay.abort().await;
        }
    }

    async fn collect(&mut self, pending: &mut HashMap<Seat, PendingRequest>) {
        while !pending.is_empty() {
            let now = Instant::now();
            let deadline = pending
                .values()
                .map(|request| request.deadline)
                .min()
                .unwrap_or(now);
            if deadline <= now {
                for (seat, elapsed) in due_requests(pending, now) {
                    if pending.remove(&seat).is_none() {
                        continue;
                    }
                    if let Some(player) = self.players.get_mut(&seat)
                        && let Ok(outcome) =
                            player
                                .adapter
                                .timeout_request(&mut self.machine, seat, elapsed)
                    {
                        let _ = self.send_ack(seat, outcome.ack);
                    }
                    self.validation_failed = true;
                    self.validation_reason.get_or_insert("timeout");
                }
                continue;
            }
            let sleep = time::sleep_until(deadline);
            tokio::pin!(sleep);
            tokio::select! {
                _ = &mut sleep => {},
                input = self.input_rx.recv() => match input {
                    Some(MatchInput::Disconnected { seat }) => { self.disconnect(seat, pending).await; },
                    Some(MatchInput::Frame { seat, bytes }) => { self.reply(seat, bytes, pending).await; },
                    None => { for seat in pending.keys().copied().collect::<Vec<_>>() { self.disconnect(seat, pending).await; } }
                }
            }
        }
    }

    async fn reply(
        &mut self,
        seat: Seat,
        bytes: Vec<u8>,
        pending: &mut HashMap<Seat, PendingRequest>,
    ) {
        let elapsed = pending
            .get(&seat)
            .copied()
            .map_or(0, |request| request.elapsed_ms_at(Instant::now()));
        let outcome = {
            let Some(player) = self.players.get_mut(&seat) else {
                return;
            };
            match player
                .adapter
                .submit_reply(&mut self.machine, seat, &bytes, elapsed)
            {
                Ok(outcome) => outcome,
                Err(_) => {
                    player.active = false;
                    self.validation_failed = true;
                    self.validation_reason.get_or_insert("protocol_error");
                    return;
                }
            }
        };
        let status = outcome.ack.status;
        let _ = self.send_ack(seat, outcome.ack);
        if self.kind == CompatKind::Validate
            && matches!(status, AckStatus::Rejected | AckStatus::Unparseable)
        {
            self.validation_failed = true;
            self.validation_reason.get_or_insert(match status {
                AckStatus::Rejected => "illegal_action",
                AckStatus::Unparseable => "protocol_error",
                _ => "protocol_error",
            });
        }
        if outcome.result.is_some() {
            pending.remove(&seat);
        }
    }

    async fn disconnect(&mut self, seat: Seat, pending: &mut HashMap<Seat, PendingRequest>) {
        if let Some(player) = self.players.get_mut(&seat) {
            player.active = false;
        }
        self.validation_failed = true;
        self.validation_reason.get_or_insert("disconnected");
        pending.remove(&seat);
        if let Ok(Some(decision)) = self.machine.current_decision() {
            let action = decision.default_action_id(seat).clone();
            if let Ok(result) = self
                .machine
                .submit_action(seat, decision.id().clone(), action)
                && result.is_resolved()
            {}
        }
    }

    async fn broadcast(&mut self, events: &[GameEvent]) -> bool {
        self.record_replay(events);
        let mut healthy = true;
        for event in events {
            for seat in self.players.keys().copied().collect::<Vec<_>>() {
                if !self.players.get(&seat).is_some_and(|player| player.active) {
                    continue;
                }
                match encode_event(event, seat, self.mode) {
                    Ok(text) if self.send(seat, Message::text(text.clone())) => {}
                    _ => {
                        healthy = false;
                        if let Some(player) = self.players.get_mut(&seat) {
                            player.active = false;
                        }
                    }
                }
            }
        }
        if events
            .iter()
            .any(|event| matches!(event, GameEvent::EndKyoku))
        {
            for player in self.players.values_mut() {
                player.adapter.reset_kyoku();
            }
        }
        self.event_cursor = self.event_cursor.saturating_add(events.len());
        healthy
    }
    fn send(&self, seat: Seat, message: Message) -> bool {
        self.players
            .get(&seat)
            .is_some_and(|player| queue_output(&player.output, message))
    }
    fn send_ack(&self, seat: Seat, ack: ActionAck) -> bool {
        serde_json::to_string(&ack)
            .ok()
            .is_some_and(|text| self.send(seat, Message::text(text)))
    }
    fn close(&self, code: u16, reason: &'static str) {
        for player in self.players.values() {
            let _ = queue_output(&player.output, close_message(code, reason));
        }
    }
}

pub(crate) async fn agent_join(
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Extension(request_id): Extension<RequestId>,
    body: Result<Bytes, BytesRejection>,
) -> Response {
    if !origin_allowed(&headers, &state) {
        return origin_not_allowed(&request_id);
    }
    if !state.participant_creation_allowed(&headers, peer.map(|p| p.0.0)) {
        return rate_limited(&request_id, state.limits().rate_window);
    }
    let token = match state.authenticate_bot(&headers, peer.map(|p| p.0.0)) {
        Ok(token) => token,
        Err(_) => return invalid_credentials(&request_id),
    };
    let body = match body {
        Ok(body) => body,
        Err(_) => return invalid_request(&request_id),
    };
    let request: AgentJoinRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(_) => return invalid_request(&request_id),
    };
    let Some(display_name) = normalize_name(&request.display_name) else {
        return invalid_request(&request_id);
    };
    let transition = tokio::select! {
        _ = state.compat.shutdown_notify.notified() => return invalid_credentials(&request_id),
        permit = state.compat.room_connections.transition.clone().acquire_owned() => {
            permit.expect("agent transition semaphore closed")
        }
    };
    if state.compat.is_shutting_down() || !state.bot_token_active(token.token_id()) {
        return invalid_credentials(&request_id);
    }
    let Some(room) = state.rooms().get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let snapshot = match room.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    if snapshot.mode.seat_count() != 4 {
        return json_response(
            StatusCode::CONFLICT,
            json!({"code":"room_mode_not_supported"}),
        );
    }
    let participant_id = ParticipantId::new(generate_ulid());
    let response = match room
        .send(RoomCommand::join_with_token(
            Participant::new(participant_id.clone(), display_name, ParticipantKind::MJAI),
            token.token_id(),
        ))
        .await
    {
        Ok(_) => json_response(
            StatusCode::CREATED,
            json!({"participant_id":participant_id.as_str(), "websocket_url":format!("/ws/v1/rooms/{join_code}/mjai?participant_id={}", participant_id.as_str())}),
        ),
        Err(error) => room_error_response(error, &request_id),
    };
    drop(transition);
    response
}

pub(crate) async fn room_mjai_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    Path(join_code): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    peer: Option<Extension<ConnectInfo<SocketAddr>>>,
    Extension(request_id): Extension<RequestId>,
) -> Response {
    if !origin_allowed(&headers, &state) {
        return origin_not_allowed(&request_id);
    }
    let token = match state.authenticate_bot(&headers, peer.map(|p| p.0.0)) {
        Ok(token) => token,
        Err(_) => return invalid_credentials(&request_id),
    };
    let Some(participant_value) = query_param(uri.query(), "participant_id") else {
        return invalid_request(&request_id);
    };
    let participant_id = ParticipantId::new(participant_value);
    let Some(room) = state.rooms().get(&join_code).await else {
        return room_not_found(&request_id);
    };
    let snapshot = match room.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return room_not_found(&request_id),
    };
    let Some(participant) = snapshot
        .participants
        .iter()
        .find(|participant| participant.id == participant_id)
    else {
        return invalid_credentials(&request_id);
    };
    if participant.kind != ParticipantKind::MJAI {
        return invalid_credentials(&request_id);
    }
    let Some(permit) = state.connection_permit() else {
        return server_busy(&request_id);
    };
    let connections = Arc::clone(&state.compat.room_connections);
    let state_for_upgrade = Arc::clone(&state);
    let room_for_upgrade = room.clone();
    let token_id = token.token_id().to_owned();
    ws.max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| async move {
            let transition = connections
                .transition
                .clone()
                .acquire_owned()
                .await
                .expect("agent transition semaphore closed");
            if state_for_upgrade.compat.is_shutting_down()
                || !state_for_upgrade.bot_token_active(&token_id)
            {
                drop(transition);
                let mut socket = socket;
                let reason = if state_for_upgrade.compat.is_shutting_down() {
                    "server_shutdown"
                } else {
                    "token_revoked"
                };
                let _ = socket
                    .send(close_message(CLOSE_SESSION_EXPIRED, reason))
                    .await;
                drop(permit);
                return;
            }
            if room_for_upgrade
                .send(RoomCommand::reconnect_agent(
                    participant_id.clone(),
                    token_id.clone(),
                ))
                .await
                .is_err()
            {
                drop(transition);
                let mut socket = socket;
                let _ = socket
                    .send(close_message(CLOSE_SESSION_EXPIRED, "session_expired"))
                    .await;
                drop(permit);
                return;
            }
            let (generation, control) = connections.register(participant_id.clone(), token_id);
            drop(transition);
            run_room_socket(
                socket,
                state_for_upgrade,
                room_for_upgrade,
                participant_id,
                generation,
                control,
                permit,
            )
            .await;
        })
}

/// Deliver disconnect cleanup after transient Room command backpressure.
async fn disconnect_room_participant(room: &RoomHandle, participant_id: &ParticipantId) {
    for attempt in 0..=ROOM_COMMAND_CAPACITY {
        let result = room
            .send(RoomCommand::disconnect(participant_id.clone()))
            .await;
        if !matches!(result, Err(RoomError::Busy)) || attempt == ROOM_COMMAND_CAPACITY {
            break;
        }
        tokio::task::yield_now().await;
    }
}

async fn teardown_room_connection(
    state: &Arc<ServerState>,
    room: &RoomHandle,
    participant_id: &ParticipantId,
    generation: u64,
) {
    let transition = state
        .compat
        .room_connections
        .transition
        .clone()
        .acquire_owned()
        .await
        .expect("agent transition semaphore closed");
    if state
        .compat
        .room_connections
        .is_current(participant_id, generation)
    {
        disconnect_room_participant(room, participant_id).await;
    }
    state
        .compat
        .room_connections
        .remove(participant_id, generation);
    drop(transition);
}

async fn run_room_socket(
    socket: WebSocket,
    state: Arc<ServerState>,
    room: RoomHandle,
    participant_id: ParticipantId,
    generation: u64,
    mut control: mpsc::Receiver<CompatControl>,
    permit: ConnectionPermit,
) {
    let mode = match room.snapshot().await {
        Ok(snapshot) => snapshot.mode,
        Err(_) => {
            teardown_room_connection(&state, &room, &participant_id, generation).await;
            drop(permit);
            return;
        }
    };
    let (mut sender, mut receiver) = socket.split();
    let (output, mut output_rx) = mpsc::channel(OUTBOUND_CAPACITY + 1);
    let mut writer = tokio::spawn(async move {
        while let Some(message) = output_rx.recv().await {
            let close = matches!(message, Message::Close(_));
            if !matches!(
                time::timeout(Duration::from_secs(1), sender.send(message)).await,
                Ok(Ok(()))
            ) || close
            {
                break;
            }
        }
    });
    let mut close_queued = false;
    let mut connection = match room.subscribe().await {
        Ok(connection) => connection,
        Err(_) => {
            writer.abort();
            teardown_room_connection(&state, &room, &participant_id, generation).await;
            drop(permit);
            return;
        }
    };
    let _ = connection.recv().await;
    let mut adapter = MjaiAdapter::new(mode);
    let mut timing = TimingBudget::new();
    let mut cursor = 0usize;
    let mut last_decision = None;
    let mut last_request_id = None;
    let mut request_time = RequestTime {
        grace_ms: 0,
        bank_ms: 0,
        deadline_ms: 0,
    };
    let mut request_opened = Instant::now();
    if !sync_room(
        &room,
        &participant_id,
        mode,
        &mut adapter,
        &mut timing,
        &mut cursor,
        &mut last_decision,
        &mut last_request_id,
        &mut request_time,
        &mut request_opened,
        &output,
    )
    .await
    {
        drop(output);
        writer.abort();
        teardown_room_connection(&state, &room, &participant_id, generation).await;
        drop(permit);
        return;
    }
    loop {
        tokio::select! {
            command = control.recv() => { if let Some(CompatControl::Close { code, reason }) = command { let _ = queue_output(&output, close_message(code, reason)); close_queued = true; } break; }
            message = receiver.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let transition = state
                        .compat
                        .room_connections
                        .transition
                        .clone()
                        .acquire_owned()
                        .await
                        .expect("agent transition semaphore closed");
                    if !state.compat.room_connections.is_current(&participant_id, generation) {
                        let _ = queue_output(&output, close_message(CLOSE_REPLACED, "connected_elsewhere"));
                        close_queued = true;
                        drop(transition);
                        break;
                    }
                    if text.len() > MAX_FRAME_BYTES {
                        let _ = queue_output(&output, close_message(CLOSE_TOO_LARGE, "message_too_large"));
                        close_queued = true;
                        drop(transition);
                        break;
                    }
                    room_reply(&room, &participant_id, mode, &mut adapter, &mut timing, request_time, request_opened, &mut last_decision, &mut last_request_id, &output, text.as_bytes()).await;
                    drop(transition);
                }
                Some(Ok(Message::Ping(payload))) => { let _ = queue_output(&output, Message::Pong(payload)); }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(_)) => {}
            },
            event = connection.recv() => { let Some(event) = event else { break; }; if matches!(event, RoomEvent::ServerShutdown | RoomEvent::RoomDeleted) { let _ = queue_output(&output, close_message(CLOSE_SESSION_EXPIRED, "server_shutdown")); close_queued = true; break; } if !sync_room(&room, &participant_id, mode, &mut adapter, &mut timing, &mut cursor, &mut last_decision, &mut last_request_id, &mut request_time, &mut request_opened, &output).await { close_queued = true; break; } }
        }
    }
    teardown_room_connection(&state, &room, &participant_id, generation).await;
    drop(output);
    if close_queued {
        let _ = time::timeout(Duration::from_secs(1), &mut writer).await;
    } else {
        writer.abort();
    }
    drop(permit);
}

async fn sync_room(
    room: &RoomHandle,
    participant_id: &ParticipantId,
    mode: GameMode,
    adapter: &mut MjaiAdapter,
    timing: &mut TimingBudget,
    cursor: &mut usize,
    last_decision: &mut Option<DecisionId>,
    last_request_id: &mut Option<u64>,
    request_time: &mut RequestTime,
    request_opened: &mut Instant,
    output: &mpsc::Sender<Message>,
) -> bool {
    let events = match room.match_events().await {
        Ok(events) => events,
        Err(_) => return false,
    };
    let projection = match room.projection(participant_id.clone()).await {
        Ok(Some(AudienceProjection::Player(projection))) => projection,
        _ => return true,
    };
    let seat = projection.viewer_seat;
    for event in events.get(*cursor..).unwrap_or_default() {
        let Ok(text) = encode_event(event, seat, mode) else {
            return false;
        };
        if !queue_output(output, Message::text(text)) {
            return false;
        }
        if matches!(event, GameEvent::EndKyoku) {
            adapter.reset_kyoku();
            timing.reset_kyoku();
        }
    }
    *cursor = events.len();
    let Some(decision) = projection.decision.as_ref() else {
        if let Some(request_id) = last_request_id.take() {
            let _ = adapter.retire_request(request_id);
        }
        *last_decision = None;
        return true;
    };
    if decision.actions.is_empty() {
        if let Some(request_id) = last_request_id.take() {
            let _ = adapter.retire_request(request_id);
        }
        *last_decision = None;
        return true;
    }
    if last_decision.as_ref() == Some(&decision.decision_id) {
        return true;
    }
    if let Some(request_id) = last_request_id.take() {
        let _ = adapter.retire_request(request_id);
    }
    let Some(wire_time) = room_request_time(decision) else {
        return true;
    };
    let Ok(request) = adapter.open_request_with_time(
        &projection,
        wire_time,
        events.get(cursor.saturating_sub(1)..).unwrap_or_default(),
    ) else {
        return false;
    };
    let Ok(text) = serde_json::to_string(&request) else {
        return false;
    };
    if !queue_output(output, Message::text(text)) {
        return false;
    }
    *last_decision = Some(decision.decision_id.clone());
    *last_request_id = Some(request.request_id);
    *request_time = request.time;
    *request_opened = Instant::now();
    true
}

fn room_request_time(decision: &PlayerDecisionProjection) -> Option<RequestTime> {
    match decision.remaining_ms {
        Some(remaining_ms) if remaining_ms > 0 => Some(RequestTime {
            grace_ms: 0,
            bank_ms: remaining_ms,
            deadline_ms: remaining_ms,
        }),
        Some(_) => None,
        None => Some(RequestTime::unlimited()),
    }
}

async fn room_reply(
    room: &RoomHandle,
    participant_id: &ParticipantId,
    mode: GameMode,
    adapter: &mut MjaiAdapter,
    timing: &mut TimingBudget,
    request_time: RequestTime,
    request_opened: Instant,
    last_decision: &mut Option<DecisionId>,
    last_request_id: &mut Option<u64>,
    output: &mpsc::Sender<Message>,
    bytes: &[u8],
) {
    let request_id = match request_id_from_frame(bytes) {
        Ok(id) => id,
        Err(_) => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(
                    None,
                    TimingOutcome {
                        elapsed_ms: 0,
                        bank_consumed_ms: 0,
                        bank_ms: timing.bank_ms(),
                        timed_out: false,
                    },
                    "action frame could not be parsed",
                )
                .ok(),
            );
            return;
        }
    };
    let disposition = adapter.peek_reply(request_id);
    let current_id = match disposition {
        ReplyDisposition::Current { request_id, .. } => request_id,
        ReplyDisposition::Stale { request_id } => {
            let _ = send_ack(output, Some(ActionAck::stale(request_id, timing.bank_ms())));
            return;
        }
        ReplyDisposition::Future { request_id } => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(
                    Some(request_id),
                    TimingOutcome {
                        elapsed_ms: 0,
                        bank_consumed_ms: 0,
                        bank_ms: timing.bank_ms(),
                        timed_out: false,
                    },
                    "unknown request_id",
                )
                .ok(),
            );
            return;
        }
        ReplyDisposition::Missing => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(
                    None,
                    TimingOutcome {
                        elapsed_ms: 0,
                        bank_consumed_ms: 0,
                        bank_ms: timing.bank_ms(),
                        timed_out: false,
                    },
                    "no pending request",
                )
                .ok(),
            );
            return;
        }
    };
    let elapsed = request_opened.elapsed().as_millis() as u64;
    let outcome = timing.preview(request_time, elapsed);
    let Ok(action) = parse_client_action(bytes, mode) else {
        let _ = send_ack(
            output,
            ActionAck::unparseable(Some(current_id), outcome, "action could not be parsed").ok(),
        );
        return;
    };
    let Ok(Some(AudienceProjection::Player(projection))) =
        room.projection(participant_id.clone()).await
    else {
        return;
    };
    let Some(decision) = projection.decision.as_ref() else {
        return;
    };
    if last_decision.as_ref() != Some(&decision.decision_id)
        || last_request_id.is_none_or(|request_id| request_id != current_id)
        || decision.actions.is_empty()
    {
        if let Some(request_id) = last_request_id.take() {
            let _ = adapter.retire_request(request_id);
        } else {
            let _ = adapter.retire_request(current_id);
        }
        *last_decision = None;
        let _ = send_ack(
            output,
            Some(ActionAck::stale(Some(current_id), timing.bank_ms())),
        );
        return;
    }
    let synthetic = Decision::new_with_timings(
        decision.decision_id.clone(),
        decision.kind,
        vec![(
            projection.viewer_seat,
            decision
                .actions
                .iter()
                .map(|action| action.action.clone())
                .collect(),
            decision.duration_ms.map(Duration::from_millis),
            decision.watchdog,
        )],
        Instant::now(),
    );
    let Ok(synthetic) = synthetic else {
        return;
    };
    let matched = match match_legal_action(mode, projection.viewer_seat, &synthetic, &action) {
        Ok(matched) => matched,
        Err(double_riichi_mjai::ProtocolError::NoMatchingAction)
        | Err(double_riichi_mjai::ProtocolError::AmbiguousAction) => {
            let legal_types = decision
                .actions
                .iter()
                .filter_map(|visible| PossibleAction::from_game_action(&visible.action).ok())
                .map(|action| match action {
                    PossibleAction::Dahai { .. } => "dahai",
                    PossibleAction::Chi { .. } => "chi",
                    PossibleAction::Pon { .. } => "pon",
                    PossibleAction::Daiminkan { .. } => "daiminkan",
                    PossibleAction::Ankan { .. } => "ankan",
                    PossibleAction::Kakan { .. } => "kakan",
                    PossibleAction::Reach { .. } => "reach",
                    PossibleAction::Hora => "hora",
                    PossibleAction::Ryukyoku => "ryukyoku",
                    PossibleAction::Kita { .. } => "kita",
                    PossibleAction::None => "none",
                })
                .map(str::to_owned)
                .collect();
            let _ = send_ack(
                output,
                ActionAck::rejected(current_id, outcome, &action, legal_types).ok(),
            );
            return;
        }
        Err(_) => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(Some(current_id), outcome, "action could not be matched")
                    .ok(),
            );
            return;
        }
    };
    let Some(visible) = decision
        .actions
        .iter()
        .find(|visible| visible.action == matched.action)
    else {
        let legal_types = decision
            .actions
            .iter()
            .filter_map(|visible| PossibleAction::from_game_action(&visible.action).ok())
            .map(|action| match action {
                PossibleAction::Dahai { .. } => "dahai",
                PossibleAction::Chi { .. } => "chi",
                PossibleAction::Pon { .. } => "pon",
                PossibleAction::Daiminkan { .. } => "daiminkan",
                PossibleAction::Ankan { .. } => "ankan",
                PossibleAction::Kakan { .. } => "kakan",
                PossibleAction::Reach { .. } => "reach",
                PossibleAction::Hora => "hora",
                PossibleAction::Ryukyoku => "ryukyoku",
                PossibleAction::Kita { .. } => "kita",
                PossibleAction::None => "none",
            })
            .map(str::to_owned)
            .collect();
        let _ = send_ack(
            output,
            ActionAck::rejected(current_id, outcome, &action, legal_types).ok(),
        );
        return;
    };
    match room
        .send(RoomCommand::submit_action(
            participant_id.clone(),
            decision.decision_id.clone(),
            visible.action_id.clone(),
        ))
        .await
    {
        Ok(RoomResponse::Action(result)) => {
            let _ = adapter.classify_reply(Some(current_id));
            *last_request_id = None;
            let outcome = timing.account(request_time, elapsed);
            let _ = send_ack(output, Some(ActionAck::accepted(current_id, outcome)));
            if result.is_resolved() {
                *last_decision = None;
            }
        }
        Ok(_) => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(Some(current_id), outcome, "action could not be applied")
                    .ok(),
            );
        }
        Err(error) => {
            let _ = send_ack(
                output,
                ActionAck::unparseable(Some(current_id), outcome, room_error_reason(&error)).ok(),
            );
        }
    }
}
fn send_ack(output: &mpsc::Sender<Message>, ack: Option<ActionAck>) -> bool {
    ack.and_then(|ack| serde_json::to_string(&ack).ok())
        .is_some_and(|text| queue_output(output, Message::text(text)))
}
fn room_error_reason(error: &RoomError) -> &'static str {
    match error {
        RoomError::Disconnected => "participant is disconnected",
        RoomError::ControllerNotInteractive => "controller is not interactive",
        _ => "action could not be applied",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdminAuthenticator, TokenState, hash_password};
    use double_riichi_core::{ActionId, DecisionKind};

    fn projected_decision(
        duration_ms: Option<u64>,
        remaining_ms: Option<u64>,
    ) -> PlayerDecisionProjection {
        PlayerDecisionProjection {
            decision_id: DecisionId::new("d1"),
            kind: DecisionKind::Turn,
            actions: vec![double_riichi_core::VisibleAction {
                action_id: ActionId::new("a1"),
                action: double_riichi_core::GameAction::Pass,
            }],
            default_action_id: ActionId::new("a1"),
            duration_ms,
            remaining_ms,
            watchdog: false,
        }
    }

    #[test]
    fn room_request_time_follows_decision_deadline_and_unlimited_sentinel() {
        assert_eq!(
            room_request_time(&projected_decision(Some(30_000), Some(29_500))),
            Some(RequestTime {
                grace_ms: 0,
                bank_ms: 29_500,
                deadline_ms: 29_500,
            })
        );
        assert_eq!(
            room_request_time(&projected_decision(None, None)),
            Some(RequestTime::unlimited())
        );
        assert_eq!(
            room_request_time(&projected_decision(Some(1), Some(0))),
            None
        );
    }

    #[test]
    fn unequal_per_seat_banks_only_due_the_earlier_request() {
        let opened_at = Instant::now();
        let pending = HashMap::from([
            (
                Seat::new(0).unwrap(),
                PendingRequest {
                    opened_at,
                    deadline: opened_at + Duration::from_millis(2),
                },
            ),
            (
                Seat::new(1).unwrap(),
                PendingRequest {
                    opened_at,
                    deadline: opened_at + Duration::from_millis(20),
                },
            ),
        ]);
        let due = due_requests(&pending, opened_at + Duration::from_millis(3));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].0, Seat::new(0).unwrap());
        assert_eq!(due[0].1, 3);
        assert!(pending.contains_key(&Seat::new(1).unwrap()));
    }

    #[test]
    fn cancelling_a_ranked_timer_advances_generation_for_old_sleepers() {
        let mut inner = CompatInner {
            queue: VecDeque::new(),
            active: HashMap::new(),
            next_ticket: 1,
            next_match: 1,
            timer_running: true,
            timer_generation: 7,
            shutting_down: false,
        };
        cancel_ranked_timer(&mut inner);
        assert!(!inner.timer_running);
        assert_eq!(inner.timer_generation, 8);
        assert_eq!(start_ranked_timer(&mut inner, 0, 1), None);
    }

    #[test]
    fn compat_full_queue_reserves_and_emits_slow_consumer_close() {
        let (sender, mut receiver) = mpsc::channel(OUTBOUND_CAPACITY + 1);
        for _ in 0..OUTBOUND_CAPACITY {
            assert!(queue_output(&sender, Message::text("event")));
        }
        assert!(!queue_output(&sender, Message::text("overflow")));
        match receiver.try_recv().unwrap() {
            Message::Text(_) => {}
            other => panic!("expected queued text before close, got {other:?}"),
        }
        for _ in 1..OUTBOUND_CAPACITY {
            assert!(matches!(receiver.try_recv(), Ok(Message::Text(_))));
        }
        match receiver.try_recv().unwrap() {
            Message::Close(Some(frame)) => {
                assert_eq!(frame.code, CLOSE_SLOW_CONSUMER);
                assert_eq!(frame.reason, "slow_consumer");
            }
            other => panic!("expected slow-consumer close, got {other:?}"),
        }
    }

    #[test]
    fn agent_join_rejects_unknown_fields() {
        assert!(
            serde_json::from_str::<AgentJoinRequest>(r#"{"display_name":"bot","unexpected":true}"#)
                .is_err()
        );
    }

    #[tokio::test]
    async fn room_teardown_retries_a_busy_disconnect_before_removing_generation() {
        let state = Arc::new(ServerState::for_tests(
            "http://127.0.0.1:3000",
            Arc::new(
                AdminAuthenticator::new(
                    "admin",
                    hash_password("correct horse battery staple").unwrap(),
                )
                .unwrap(),
            ),
            RoomRegistry::with_max_rooms(1),
        ));
        let room = state
            .rooms()
            .create(double_riichi_core::RoomConfig::new(
                "Room",
                GameMode::FourPlayerRedEast,
                double_riichi_core::CharacterCatalog::starter(),
            ))
            .await
            .unwrap();
        let participant = Participant::new("agent", "Agent", ParticipantKind::MJAI);
        room.send(RoomCommand::join_with_token(participant.clone(), "token"))
            .await
            .unwrap();
        let generation = state
            .compat
            .room_connections
            .register(participant.id.clone(), "token".to_owned())
            .0;
        let mut queued = Vec::new();
        for _ in 0..=ROOM_COMMAND_CAPACITY {
            match room.try_send(RoomCommand::GetSnapshot) {
                Ok(reply) => queued.push(reply),
                Err(RoomError::Busy) => break,
                Err(error) => panic!("failed to fill Room command queue: {error}"),
            }
        }
        assert_eq!(queued.len(), ROOM_COMMAND_CAPACITY);

        teardown_room_connection(&state, &room, &participant.id, generation).await;

        let snapshot = loop {
            match room.snapshot().await {
                Ok(snapshot) => break snapshot,
                Err(RoomError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("failed to read Room after teardown: {error}"),
            }
        };
        assert_eq!(
            snapshot
                .participants
                .iter()
                .find(|entry| entry.id == participant.id)
                .expect("participant remains in Room")
                .presence,
            double_riichi_core::Presence::Disconnected
        );
        assert!(
            !state
                .compat
                .room_connections
                .is_current(&participant.id, generation)
        );
        state.shutdown().await;
    }

    #[test]
    fn replacement_disconnect_cannot_remove_new_generation() {
        let connections = AgentConnections::default();
        let participant = ParticipantId::new("agent");
        let (old_generation, _old_control) =
            connections.register(participant.clone(), "token".to_owned());
        let (new_generation, _new_control) =
            connections.register(participant.clone(), "token".to_owned());
        assert!(!connections.is_current(&participant, old_generation));
        assert!(connections.is_current(&participant, new_generation));
        connections.remove(&participant, old_generation);
        assert!(connections.is_current(&participant, new_generation));
    }

    #[tokio::test]
    async fn cancelled_match_lease_releases_active_capacity() {
        let state = Arc::new(CompatState::new(1, 1));
        state.active_matches.store(1, Ordering::Release);
        let lease = ActiveMatchLease {
            state: state.clone(),
            match_id: 1,
            released: Arc::new(AtomicBool::new(false)),
        };
        drop(lease);
        assert_eq!(state.active_count().await, 0);
    }

    #[tokio::test]
    async fn aborted_compat_replay_cleans_partial_file_and_metadata() {
        let root =
            std::env::temp_dir().join(format!("double-riichi-compat-abort-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let storage = Arc::new(Storage::connect(&root).await.unwrap());
        let mode = GameMode::FourPlayerRedHalf;
        let players = (0..4)
            .map(|seat| {
                Participant::new(
                    format!("abort{seat}"),
                    format!("Abort {seat}"),
                    ParticipantKind::MJAI,
                )
            })
            .collect::<Vec<_>>();
        let writer = ReplayWriter::new(storage.replay_root(), "abortmatch", mode).unwrap();
        let part = writer.part_path().to_path_buf();
        storage
            .open_ranked_match(
                "abortmatch",
                mode,
                1,
                &writer.relative_path_string(),
                &players,
            )
            .await
            .unwrap();
        CompatReplay {
            writer: Some(writer),
            storage: Some(storage.clone()),
            replay_health: Arc::new(AtomicBool::new(false)),
            cleanup_tasks: Arc::new(Mutex::new(Vec::new())),
            metadata_open: true,
            failed: false,
            match_id: "abortmatch".to_owned(),
        }
        .abort()
        .await;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM matches WHERE match_id = 'abortmatch'")
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
        assert!(!part.exists());
        storage.close().await;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn failed_compat_replay_cleans_partial_file_and_metadata() {
        let root = std::env::temp_dir().join(format!(
            "double-riichi-compat-failure-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let storage = Arc::new(Storage::connect(&root).await.unwrap());
        let mode = GameMode::FourPlayerRedHalf;
        let players = (0..4)
            .map(|seat| {
                Participant::new(
                    format!("failure{seat}"),
                    format!("Failure {seat}"),
                    ParticipantKind::MJAI,
                )
            })
            .collect::<Vec<_>>();
        let writer =
            ReplayWriter::with_failure_after_writes(storage.replay_root(), "failurematch", mode, 0)
                .unwrap();
        let part = writer.part_path().to_path_buf();
        storage
            .open_ranked_match(
                "failurematch",
                mode,
                1,
                &writer.relative_path_string(),
                &players,
            )
            .await
            .unwrap();
        let replay_health = Arc::new(AtomicBool::new(false));
        let mut replay = CompatReplay {
            writer: Some(writer),
            storage: Some(storage.clone()),
            replay_health: replay_health.clone(),
            cleanup_tasks: Arc::new(Mutex::new(Vec::new())),
            metadata_open: true,
            failed: false,
            match_id: "failurematch".to_owned(),
        };
        replay.record(&[GameEvent::StartGame {
            names: Some(vec![
                "East".into(),
                "South".into(),
                "West".into(),
                "North".into(),
            ]),
            id: Some("failurematch".into()),
        }]);
        assert!(replay.failed);
        replay.finish(None).await;
        assert!(replay_health.load(Ordering::Acquire));
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM matches WHERE match_id = 'failurematch'")
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
        assert!(!part.exists());
        storage.close().await;
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn forced_shutdown_waits_for_detached_replay_cleanup() {
        let root = std::env::temp_dir().join(format!(
            "double-riichi-compat-shutdown-cleanup-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let storage = Arc::new(Storage::connect(&root).await.unwrap());
        let mode = GameMode::FourPlayerRedHalf;
        let players = (0..4)
            .map(|seat| {
                Participant::new(
                    format!("shutdown{seat}"),
                    format!("Shutdown {seat}"),
                    ParticipantKind::MJAI,
                )
            })
            .collect::<Vec<_>>();
        let writer = ReplayWriter::new(storage.replay_root(), "shutdownmatch", mode).unwrap();
        storage
            .open_ranked_match(
                "shutdownmatch",
                mode,
                1,
                &writer.relative_path_string(),
                &players,
            )
            .await
            .unwrap();
        let state = Arc::new(CompatState::new(1, 1));
        drop(CompatReplay {
            writer: Some(writer),
            storage: Some(storage.clone()),
            replay_health: state.replay_degraded.clone(),
            cleanup_tasks: state.replay_cleanup_tasks.clone(),
            metadata_open: true,
            failed: false,
            match_id: "shutdownmatch".to_owned(),
        });
        state.shutdown().await;
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM matches WHERE match_id = 'shutdownmatch'")
                .fetch_one(storage.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
        storage.close().await;
        let _ = std::fs::remove_dir_all(&root);
    }

    fn queued_bot(ticket: u64) -> QueuedBot {
        let (assignment, _receiver) = oneshot::channel();
        QueuedBot {
            ticket,
            token: BotTokenRecord::new(
                format!("token-{ticket}"),
                format!("Bot {ticket}"),
                [ticket as u8; 32],
                TokenState::Active,
                0,
                None,
            ),
            display_name: format!("Bot {ticket}"),
            assignment,
            permit: None,
            replay_root: None,
            replay_storage: None,
        }
    }

    #[tokio::test]
    async fn ranked_waiters_restart_after_capacity_release_and_ignore_old_sleepers() {
        let state = Arc::new(CompatState::new(1, 128));
        {
            let mut inner = state.inner.lock().await;
            inner.queue.push_back(queued_bot(1));
            inner.active.insert(
                1,
                ActiveCompat {
                    controls: Vec::new(),
                    task: None,
                    released: Arc::new(AtomicBool::new(false)),
                },
            );
            state.active_matches.store(1, Ordering::Release);
            inner.timer_running = true;
            inner.timer_generation = 7;
        }
        assert_eq!(state.active_count().await, 1);

        assert!(state.fill_ranked(7).await);
        {
            let inner = state.inner.lock().await;
            assert_eq!(inner.queue.len(), 1);
            assert!(!inner.timer_running);
            assert_eq!(inner.timer_generation, 8);
        }

        state.finish(1).await;
        assert_eq!(state.active_count().await, 0);
        {
            let inner = state.inner.lock().await;
            assert_eq!(inner.queue.len(), 1);
            assert!(inner.timer_running);
            assert_eq!(inner.timer_generation, 9);
        }
        assert!(state.fill_ranked(8).await);
        assert_eq!(state.inner.lock().await.queue.len(), 1);
        state.shutdown().await;
    }
}
