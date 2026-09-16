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

export function animationEvents(value: unknown): unknown[] {
  if (Array.isArray(value)) return value;
  if (!value || typeof value !== "object") return [];
  const object = value as Record<string, unknown>;
  if (Array.isArray(object.events)) return object.events;
  if (object.event !== undefined) return [object.event];
  return [value];
}

export function enqueueAnimationEvents(
  current: AnimationItem[],
  events: unknown[],
  nextId = current.length,
): AnimationQueueResult {
  const mapped = events.flatMap((event, index) => {
    const kind = animationKindForEvent(event);
    return kind ? [{ id: nextId + index, kind, event }] : [];
  });
  if (current.length + mapped.length > ANIMATION_QUEUE_CAP) return { queue: [], overflow: true };
  return { queue: current.concat(mapped), overflow: false };
}

export function enqueueAnimation(current: AnimationItem[], item: Omit<AnimationItem, "id">): AnimationQueueResult {
  if (current.length >= ANIMATION_QUEUE_CAP) return { queue: [], overflow: true };
  return { queue: current.concat({ ...item, id: current.length }), overflow: false };
}

export function clearAnimationQueue(): AnimationItem[] {
  return [];
}
