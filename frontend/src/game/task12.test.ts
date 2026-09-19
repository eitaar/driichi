import { beforeEach, describe, expect, it, vi } from "vitest";
import { actionCandidates } from "./actions";
import { animationKindForEvent, animationVisualForKind, enqueueAnimationEvents, ANIMATION_QUEUE_CAP } from "./animation";
import { AudioManager, loadAudioSettings, saveAudioSettings, voiceAssetPath } from "./audio";
import { preloadRosterAssets } from "./gameplay";
import { seatPositions } from "./orientation";
import { useGameStore } from "./store";
import { tileAssetUrl, tileFileName, tileLabel } from "./tiles";
import { portraitFromEvents } from "./gameplay";
import { displayPlayerName } from "./table-art";
import { effectDuration } from "./table-effects";

beforeEach(() => {
  useGameStore.getState().reset();
});

describe("Task 12 table invariants", () => {
  it("rotates three and four player seats without inventing a fourth seat", () => {
    expect(seatPositions("3p-red-east", 1)).toEqual([
      { seat: 1, position: "bottom" },
      { seat: 2, position: "right" },
      { seat: 0, position: "left" },
    ]);
    expect(seatPositions("4p-red-east", 2).map(({ seat, position }) => [seat, position])).toEqual([
      [2, "bottom"], [3, "right"], [0, "top"], [1, "left"],
    ]);
  });

  it("maps physical red fives to vendored art", () => {
    expect(tileFileName(16)).toBe("Man5-Dora.svg");
    expect(tileFileName(52)).toBe("Pin5-Dora.svg");
    expect(tileFileName(88)).toBe("Sou5-Dora.svg");
    expect(tileAssetUrl(16)).toContain("Regular/Man5-Dora.svg");
    expect(tileLabel(16)).toContain("5m");
  });

  it("removes transition time under Reduced Motion", () => {
    expect(effectDuration("discard", true)).toBe(0);
    expect(effectDuration("win", true)).toBe(0);
    expect(effectDuration("discard", false)).toBe(240);
    expect(effectDuration("win", false)).toBe(520);
  });

  it("keeps exactly 64 animations and cancels on the next item", () => {
    const events = Array.from({ length: ANIMATION_QUEUE_CAP }, () => ({ dahai: { actor: 0, tile: 0 } }));
    const exact = enqueueAnimationEvents([], events);
    expect(exact.overflow).toBe(false);
    expect(exact.queue).toHaveLength(ANIMATION_QUEUE_CAP);
    const overflow = enqueueAnimationEvents(exact.queue, [{ tsumo: { actor: 0, tile: 1 } }]);
    expect(overflow.overflow).toBe(true);
    expect(overflow.queue).toEqual([]);
    expect(animationKindForEvent({ hora: { actor: 0, target: 0, han: 5 } })).toBe("win");
  });

  it("keeps the first queued animation stable across unrelated updates", () => {
    const room = (revision: number) => ({ revision }) as never;
    useGameStore.getState().receiveSnapshot(room(1), {
      audience: "player", viewer_seat: 0, mode: "4p-red-east", players: [], decision: null,
    });
    useGameStore.getState().receiveUpdate(room(2), undefined, { type: "game_update", event: { dahai: { actor: 0, tile: 1 } } });
    const queue = useGameStore.getState().animationQueue;
    expect(queue).toHaveLength(1);
    useGameStore.getState().receiveUpdate(room(3), undefined, { type: "room_update" });
    expect(useGameStore.getState().animationQueue).toBe(queue);
    expect(animationVisualForKind(queue[0].kind).shape).toBe("square");
  });

  it("keeps Riichi discard candidates available for the persistent legal highlight", () => {
    const grouped = actionCandidates({ decision_id: "d", kind: "turn", actions: [
      { action_id: "r1", action: { riichi_discard: { tile: 52 } } },
    ] });
    expect(grouped.riichiDiscard).toHaveLength(1);
    expect(animationVisualForKind("draw").shape).not.toBe(animationVisualForKind("discard").shape);
  });

  it("submits only the authoritative action IDs and never mutates projection optimistically", () => {
    const sent: unknown[] = [];
    useGameStore.getState().receiveSnapshot(null, {
      audience: "player", viewer_seat: 0, mode: "4p-red-east", players: [],
      decision: { decision_id: "d1", kind: "turn", actions: [{ action_id: "a1", action: { discard: { tile: 16, tsumogiri: false } } }] },
    });
    expect(useGameStore.getState().submitAction("d1", "a1", (value) => sent.push(value))).toBe(true);
    expect(sent).toEqual([{ type: "submit_action", decision_id: "d1", action_id: "a1" }]);
    expect(useGameStore.getState().projection?.decision?.decision_id).toBe("d1");
    expect(useGameStore.getState().submitAction("d1", "a1", (value) => sent.push(value))).toBe(false);
    useGameStore.getState().receiveActionResult({ decision_id: "d1", action_id: "a1", status: "rejected", code: "illegal_action" });
    expect(useGameStore.getState().pendingAction).toBeNull();
    useGameStore.getState().receiveSnapshot(null, {
      audience: "player", viewer_seat: 0, mode: "4p-red-east", players: [],
      decision: { decision_id: "d2", kind: "turn", actions: [{ action_id: "a2", action: { discard: { tile: 16, tsumogiri: false } } }] },
    });
    expect(useGameStore.getState().submitAction("d2", "a2", (value) => sent.push(value))).toBe(true);
    useGameStore.getState().receiveActionResult({ decision_id: "d2", action_id: "other", status: "accepted" });
    expect(useGameStore.getState().pendingAction).toEqual({ decisionId: "d2", actionId: "a2" });
    useGameStore.getState().receiveActionResult({ decision_id: "d2", action_id: "a2", status: "accepted" });
    expect(useGameStore.getState().pendingAction).toBeNull();
  });

  it("groups compound candidates and persists safe audio settings", () => {
    const actions = actionCandidates({ decision_id: "d", kind: "response", actions: [
      { action_id: "a1", action: { chi: { called: 1 } } },
      { action_id: "a2", action: { chi: { called: 2 } } },
      { action_id: "a3", action: "pass" },
    ] });
    expect(actions.candidates.get("chi")).toHaveLength(2);
    expect(actions.simple).toHaveLength(1);
    expect(voiceAssetPath("red", "kan")).toBe("/assets/characters/red/voices/kan.ogg");
    let stored = "";
    const storage = { getItem: () => stored, setItem: (_key: string, value: string) => { stored = value; } } as unknown as Storage;
    saveAudioSettings({ master: 0.5, sfx: 0.4, voice: 0.3, voiceEnabled: false }, storage);
    expect(loadAudioSettings(storage)).toEqual({ master: 0.5, sfx: 0.4, voice: 0.3, voiceEnabled: false });
  });

  it("bounds long table labels without changing the source Display Name", () => {
    const name = "A very long participant display name";
    expect(displayPlayerName(name)).toBe("A very long partic…");
    expect(name).toBe("A very long participant display name");
  });

  it("derives a visible Mangan result from the projected resolution event", () => {
    const room = { roster: [{ seat: 0, participant_id: "p1", display_name: "Mika", kind: "human", character_id: "player-red", controller: "interactive" }] } as never;
    expect(portraitFromEvents([{ hora: { actor: 0, target: 1, han: 5, fu: 30, delta: [8000, -8000] } }], room)).toMatchObject({ displayName: "Mika", result: "Ron", limit: "Mangan" });
  });

  it("advances the voice queue when playback throws synchronously", () => {
    const first = { play: vi.fn(() => { throw new Error("blocked"); }), pause: vi.fn(), volume: 0, preload: "", onended: null, src: "", muted: false } as unknown as HTMLAudioElement;
    const second = { play: vi.fn(() => Promise.resolve()), pause: vi.fn(), volume: 0, preload: "", onended: null, src: "", muted: false } as unknown as HTMLAudioElement;
    const factory = vi.fn().mockReturnValueOnce(first).mockReturnValueOnce(second);
    const manager = new AudioManager(factory, { master: 1, sfx: 1, voice: 1, voiceEnabled: true });
    manager.playVoice("red", "ron");
    manager.playVoice("red", "chi");
    expect(factory).toHaveBeenCalledTimes(2);
    expect(second.play).toHaveBeenCalledTimes(1);
    manager.destroy();
  });

  it("recovers from a character decode that never settles after three seconds", async () => {
    vi.useFakeTimers();
    class HangingImage {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(_value: string) {}
    }
    vi.stubGlobal("Image", HangingImage);
    let settled = false;
    const result = preloadRosterAssets(["red"]).then((loaded) => { settled = true; return loaded; });
    await vi.advanceTimersByTimeAsync(2_999);
    expect(settled).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await expect(result).resolves.toEqual({ red: false });
    vi.useRealTimers();
  });

  it("fails media playback silently and force-stops at ten seconds", () => {
    vi.useFakeTimers();
    const audio = { play: vi.fn().mockRejectedValue(new Error("blocked")), pause: vi.fn(), volume: 0, preload: "", onended: null, src: "", muted: false } as unknown as HTMLAudioElement;
    const manager = new AudioManager(() => audio, { master: 1, sfx: 1, voice: 1, voiceEnabled: true });
    expect(() => manager.playVoice("red", "ron")).not.toThrow();
    vi.advanceTimersByTime(10_000);
    expect(audio.pause).toHaveBeenCalled();
    manager.destroy();
    vi.useRealTimers();
  });
});
