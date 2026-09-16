export const ANIMATION_QUEUE_CAP = 64;
export const MAX_ANIMATION_QUEUE = ANIMATION_QUEUE_CAP;

export type AnimationKind =
  | "draw"
  | "discard"
  | "call"
  | "win"
  | "riichi"
  | "kyoku_start"
  | "kyoku_end"
  | "score_change";

export interface AnimationItem {
  id: number;
  kind: AnimationKind;
  event: unknown;
}

export type AnimationShape = "circle" | "square" | "diamond" | "ring" | "line";

export interface AnimationVisual {
  color: number;
  alpha: number;
  duration: number;
  radius: number;
  scale: number;
  shape: AnimationShape;
}

export interface AnimationQueueResult {
  queue: AnimationItem[];
  overflow: boolean;
}

function eventName(event: unknown): string | undefined {
  if (typeof event === "string") return event.toLowerCase();
  if (!event || typeof event !== "object") return undefined;
  const object = event as Record<string, unknown>;
  if (typeof object.type === "string") return object.type.toLowerCase();
  if (object.event && typeof object.event === "object") return eventName(object.event);
  if (Array.isArray(object.events)) return undefined;
  return Object.keys(object)[0]?.toLowerCase();
}

export function animationKindForEvent(event: unknown): AnimationKind | undefined {
  switch (eventName(event)) {
    case "tsumo": return "draw";
    case "dahai": return "discard";
    case "chi":
    case "pon":
    case "daiminkan":
    case "kakan":
    case "ankan":
    case "kita": return "call";
    case "hora": return "win";
    case "reach":
    case "reach_accepted": return "riichi";
    case "start_kyoku": return "kyoku_start";
    case "end_kyoku": return "kyoku_end";
    case "score_change":
    case "scores_changed": return "score_change";
    default: return undefined;
  }
}

/** Restrained but distinct table transitions for each authoritative event class. */
export function animationVisualForKind(kind: AnimationKind): AnimationVisual {
  switch (kind) {
    case "draw": return { color: 0x8fbfa9, alpha: 0.28, duration: 220, radius: 15, scale: 0.7, shape: "circle" };
    case "discard": return { color: 0xd26c63, alpha: 0.26, duration: 240, radius: 13, scale: 0.55, shape: "square" };
    case "call": return { color: 0x84a9c0, alpha: 0.27, duration: 300, radius: 17, scale: 0.65, shape: "diamond" };
    case "win": return { color: 0xe9c27b, alpha: 0.35, duration: 520, radius: 28, scale: 0.8, shape: "ring" };
    case "riichi": return { color: 0xd26c63, alpha: 0.32, duration: 360, radius: 23, scale: 0.9, shape: "line" };
    case "kyoku_start": return { color: 0x8daf98, alpha: 0.22, duration: 420, radius: 24, scale: 0.5, shape: "ring" };
    case "kyoku_end": return { color: 0x9b8bb5, alpha: 0.24, duration: 420, radius: 24, scale: 1.1, shape: "square" };
    case "score_change": return { color: 0xe9c27b, alpha: 0.24, duration: 280, radius: 18, scale: 0.8, shape: "line" };
  }
}

export function animationEvents(value: unknown): unknown[] {
  if (Array.isArray(value)) return value.flatMap(animationEvents);
  if (!value || typeof value !== "object") return [];
  const object = value as Record<string, unknown>;
  if (Array.isArray(object.events)) return object.events.flatMap(animationEvents);
  if (object.event !== undefined) return animationEvents(object.event);
  return [value];
}

function nextAnimationId(current: AnimationItem[]): number {
  return current.reduce((next, item) => Math.max(next, item.id + 1), 0);
}

export function enqueueAnimationEvents(
  current: AnimationItem[],
  events: unknown[],
  nextId = nextAnimationId(current),
): AnimationQueueResult {
  const mapped = events.flatMap((event, index) => {
    const kind = animationKindForEvent(event);
    return kind ? [{ id: nextId + index, kind, event }] : [];
  });
  if (mapped.length === 0) return { queue: current, overflow: false };
  if (current.length + mapped.length > ANIMATION_QUEUE_CAP) return { queue: [], overflow: true };
  return { queue: current.concat(mapped), overflow: false };
}

export function enqueueAnimation(current: AnimationItem[], item: Omit<AnimationItem, "id">): AnimationQueueResult {
  if (current.length >= ANIMATION_QUEUE_CAP) return { queue: [], overflow: true };
  return { queue: current.concat({ ...item, id: nextAnimationId(current) }), overflow: false };
}

export function clearAnimationQueue(): AnimationItem[] {
  return [];
}
