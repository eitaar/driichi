import type { ProjectedDecision, VisibleAction } from "./types";
import { tileLabel } from "./tiles";

export type ActionKind =
  | "discard"
  | "riichi_discard"
  | "chi"
  | "pon"
  | "daiminkan"
  | "ankan"
  | "kakan"
  | "kan"
  | "nuki"
  | "tsumo"
  | "ron"
  | "pass"
  | "abortive_draw"
  | "unknown";

export function actionKind(action: unknown): ActionKind {
  if (typeof action === "string") {
    const normalized = action.toLowerCase();
    if (normalized === "tsumo" || normalized === "ron" || normalized === "pass") return normalized;
    if (normalized === "kan") return "kan";
    return normalized as ActionKind;
  }
  if (!action || typeof action !== "object") return "unknown";
  const key = Object.keys(action as Record<string, unknown>)[0]?.toLowerCase();
  if (!key) return "unknown";
  if (key === "daiminkan" || key === "ankan" || key === "kakan") return key;
  if (key === "tsumo" || key === "ron" || key === "pass" || key === "discard" || key === "riichi_discard" || key === "chi" || key === "pon" || key === "nuki" || key === "abortive_draw") return key;
  return "unknown";
}

export function actionPayload(action: unknown): Record<string, unknown> {
  if (!action || typeof action !== "object") return {};
  const key = Object.keys(action as Record<string, unknown>)[0];
  const value = (action as Record<string, unknown>)[key];
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

export function actionTile(action: unknown): number | undefined {
  const payload = actionPayload(action);
  return typeof payload.tile === "number" ? payload.tile : undefined;
}

export function actionIsDiscard(action: VisibleAction): boolean {
  return actionKind(action.action) === "discard";
}

export function actionIsRiichiDiscard(action: VisibleAction): boolean {
  return actionKind(action.action) === "riichi_discard";
}

export function actionGroupLabel(kind: ActionKind): string {
  if (kind === "daiminkan" || kind === "ankan" || kind === "kakan" || kind === "kan") return "Kan";
  if (kind === "riichi_discard") return "Riichi";
  if (kind === "nuki") return "Kita";
  if (kind === "abortive_draw") return "Abortive draw";
  return kind === "unknown" ? "Action" : kind[0].toUpperCase() + kind.slice(1);
}

export function describeAction(action: VisibleAction): string {
  const kind = actionKind(action.action);
  const payload = actionPayload(action.action);
  const tile = typeof payload.tile === "number" ? ` ${tileLabel(payload.tile)}` : "";
  const called = typeof payload.called === "number" ? ` ${tileLabel(payload.called)}` : "";
  if (kind === "discard" || kind === "riichi_discard" || kind === "nuki") return `${actionGroupLabel(kind)}${tile}`;
  if (kind === "chi" || kind === "pon" || kind === "daiminkan") return `${actionGroupLabel(kind)}${called}`;
  if (kind === "kakan") return `Kan${called}`;
  if (kind === "ankan") return "Kan closed";
  return actionGroupLabel(kind);
}

export function actionCandidates(decision: ProjectedDecision | null | undefined): {
  simple: VisibleAction[];
  candidates: Map<"chi" | "pon" | "kan" | "nuki", VisibleAction[]>;
  discard: VisibleAction[];
  riichiDiscard: VisibleAction[];
} {
  const simple: VisibleAction[] = [];
  const candidates = new Map<"chi" | "pon" | "kan" | "nuki", VisibleAction[]>();
  const discard: VisibleAction[] = [];
  const riichiDiscard: VisibleAction[] = [];
  for (const action of decision?.actions ?? []) {
    const kind = actionKind(action.action);
    if (kind === "discard") discard.push(action);
    else if (kind === "riichi_discard") riichiDiscard.push(action);
    else if (kind === "chi" || kind === "pon" || kind === "nuki") {
      const list = candidates.get(kind) ?? [];
      list.push(action);
      candidates.set(kind, list);
    } else if (kind === "daiminkan" || kind === "ankan" || kind === "kakan" || kind === "kan") {
      const list = candidates.get("kan") ?? [];
      list.push(action);
      candidates.set("kan", list);
    } else if (kind === "ron" || kind === "tsumo" || kind === "pass" || kind === "abortive_draw") simple.push(action);
  }
  return { simple, candidates, discard, riichiDiscard };
}
