export interface DoraSheenClock {
  sweep: { value: number };
  strength: { value: number };
  listeners: Set<() => void>;
}

export function doraSheenFrame(nowMs: number, enabled: boolean, reduced: boolean, visible: boolean) {
  if (!enabled || !visible || !Number.isFinite(nowMs) || nowMs < 0) {
    return { phase: -10, strength: 0, waitMs: null };
  }
  if (reduced) return { phase: 0.5, strength: 0.28, waitMs: null };
  const elapsed = nowMs % 2000;
  return elapsed < 600
    ? { phase: elapsed / 600, strength: 0.85, waitMs: 0 }
    : { phase: -10, strength: 0, waitMs: 2000 - elapsed };
}

// One absolute clock drives both surface uniforms and DOM; gaps use a timer, not frames.
export function startDoraSheen(host: HTMLElement, clock: DoraSheenClock, enabled: boolean, reduced: boolean): () => void {
  let frame: number | null = null;
  let timer: number | null = null;
  const cancel = () => {
    if (frame !== null) window.cancelAnimationFrame(frame);
    if (timer !== null) window.clearTimeout(timer);
    frame = timer = null;
  };
  const tick = () => {
    cancel();
    const { phase, strength, waitMs } = doraSheenFrame(performance.now(), enabled, reduced, !document.hidden);
    clock.sweep.value = phase;
    clock.strength.value = strength;
    host.style.setProperty("--dora-sheen-shift", `${(-1.15 + 2.3 * phase) * 50}%`);
    host.style.setProperty("--dora-sheen-opacity", String(strength));
    host.dataset.sheenPhase = String(phase);
    for (const notify of clock.listeners) notify();
    if (waitMs === 0) frame = window.requestAnimationFrame(tick);
    else if (waitMs !== null) timer = window.setTimeout(tick, Math.ceil(waitMs));
  };
  document.addEventListener("visibilitychange", tick);
  tick();
  return () => {
    cancel();
    document.removeEventListener("visibilitychange", tick);
    clock.sweep.value = -10;
    clock.strength.value = 0;
    host.style.removeProperty("--dora-sheen-shift");
    host.style.removeProperty("--dora-sheen-opacity");
    delete host.dataset.sheenPhase;
    for (const notify of clock.listeners) notify();
  };
}
