use std::{
    env,
    io::{self, IsTerminal},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use double_riichi_server::{RuntimeConfig, ServerState, hash_password_for_cli, server_router};

#[tokio::main]
async fn main() {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("hash-password") if arguments.next().is_none() => {
            if let Err(error) = hash_password_command() {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("--version") if arguments.next().is_none() => {
            println!("driichi {} (unknown)", env!("CARGO_PKG_VERSION"));
        }
        Some("--config") => {
            let Some(path) = arguments.next() else {
                eprintln!("usage: driichi --config <path>");
                std::process::exit(2);
            };
            if arguments.next().is_some() {
                eprintln!("usage: driichi --config <path>");
                std::process::exit(2);
            }
            if let Err(error) = run_server(PathBuf::from(path)).await {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        None => {
            if let Err(error) = run_server(PathBuf::from("config.toml")).await {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("usage: driichi [--config <path>] | driichi hash-password");
            std::process::exit(2);
        }
    }
}

async fn run_server(path: PathBuf) -> Result<(), String> {
    let config = RuntimeConfig::from_path(&path).map_err(|error| error.to_string())?;
    let bind: SocketAddr = config
        .bind
        .parse()
        .map_err(|_| "bind is invalid".to_owned())?;
    let state = ServerState::from_config(config)
        .await
        .map_err(|error| error.to_string())?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|_| "could not bind server socket".to_owned())?;
    let state = Arc::new(state);
    let shutdown_seconds = state.shutdown_seconds();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let mut server = Box::pin(
        axum::serve(
            listener,
            server_router(state.clone()).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = stop_rx.await;
        })
        .into_future(),
    );
    tokio::select! {
        result = &mut server => {
            return result.map_err(|_| "server stopped unexpectedly".to_owned());
        }
        _ = shutdown_signal() => {}
    }
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(shutdown_seconds);
    state.begin_shutdown();
    let _ = stop_tx.send(());
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(shutdown_seconds),
        state.shutdown(),
    )
    .await;
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    match tokio::time::timeout(remaining, &mut server).await {
        Ok(result) => result.map_err(|_| "server stopped unexpectedly".to_owned()),
        Err(_) => Ok(()),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut signal) = signal(SignalKind::terminate()) {
            let _ = signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

fn hash_password_command() -> Result<(), String> {
    let password = read_secret("Password: ")?;
    let confirmation = read_secret("Confirm password: ")?;
    match hash_password_for_cli(&password, &confirmation)
        .map_err(|_| "password is invalid".to_owned())?
    {
        Some(hash) => {
            println!("{hash}");
            Ok(())
        }
        None => Err("password confirmation does not match".to_owned()),
    }
}

fn read_secret(prompt: &str) -> Result<String, String> {
    eprint!("{prompt}");
    if io::stdin().is_terminal() {
        rpassword::read_password().map_err(|_| "could not read password".to_owned())
    } else {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        rpassword::read_password_from_bufread(&mut input)
            .map_err(|_| "could not read password".to_owned())
    }
}
