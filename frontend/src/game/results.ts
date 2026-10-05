import type { ResultStepId } from "./types";

const MAX_ARRAY_LENGTH = 256;
const MAX_ID_LENGTH = 128;
const MAX_NAME_LENGTH = 256;
const MAX_EVENT_INDEX = 10_000_000;
const MAX_U32 = 4_294_967_295;
const MIN_I32 = -2_147_483_648;
const MAX_I32 = 2_147_483_647;

export type ScoreLimit =
  | "Mangan"
  | "Haneman"
  | "Baiman"
  | "Sanbaiman"
  | "CountedYakuman";

export interface YakuValue {
  id: number;
  name: string;
  han?: number;
  yakuman_multiplier?: number;
}

export type WinEvaluation =
  | { Ordinary: { han: number; fu: number; limit?: ScoreLimit } }
  | { Yakuman: { multiplier: number } };

export type ResultMeldKind = "Chi" | "Pon" | "Daiminkan" | "Ankan" | "Kakan";

export interface ResultMeld {
  kind: ResultMeldKind;
  tiles: number[];
  opened: boolean;
  from_who?: number;
  called_tile?: number;
}

export interface WinResult {
  event_index: number;
  winner: string;
  seat: number;
  target: number;
  tsumo: boolean;
  concealed: number[];
  melds: ResultMeld[];
  winning_tile: number;
  winning_tile_index: number;
  yaku: YakuValue[];
  evaluation: WinEvaluation;
  win_points: number;
}

export interface ScoreChange {
  participant_id: string;
  seat: number;
  before: number;
  delta: number;
  after: number;
}

export interface KyokuSettlement {
  changes: ScoreChange[];
  pot_before: number;
  pot_after: number;
  draw_reason?: string;
}

export type KyokuEndContent =
  | { Win: WinResult }
  | { Settlement: KyokuSettlement }
  | { Draw: KyokuSettlement };

export interface KyokuEndStep {
  id: ResultStepId;
  content: KyokuEndContent;
  published_at_ms: number;
  deadline_at_ms: number;
  remaining_ms: number;
  required: string[];
  confirmed: string[];
}

export interface MatchPlayerResult {
  participant_id: string;
  display_name: string;
  kind: "Human" | "MJAI" | "MCP" | "BuiltInBot";
  seat: number;
  final_score: number;
  rank: number;
}

/** The validated wire MatchResult; ranking is never reconstructed in the client. */
export interface FinalStandings {
  mode: string;
  players: MatchPlayerResult[];
  final_scores: number[];
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function integer(value: unknown, min: number, max: number): number | null {
  return typeof value === "number"
    && Number.isSafeInteger(value)
    && value >= min
    && value <= max
    ? value
    : null;
}

function boundedString(value: unknown, maxLength: number, trim = true): string | null {
  if (typeof value !== "string" || value.length === 0 || value.length > maxLength) return null;
  if (trim && value.trim().length === 0) return null;
  return value;
}

function participantId(value: unknown): string | null {
  return boundedString(value, MAX_ID_LENGTH);
}

function modeSeatCount(mode: string): number | null {
  switch (mode) {
    case "4p-red-east":
    case "4p-red-half":
    case "FourPlayerRedEast":
    case "FourPlayerRedHalf":
      return 4;
    case "3p-red-east":
    case "3p-red-half":
    case "ThreePlayerRedEast":
    case "ThreePlayerRedHalf":
      return 3;
    default:
      return null;
  }
}

function validTile(tile: unknown, seatCount: number): number | null {
  const id = integer(tile, 0, 135);
  if (id === null) return null;
  // Rust's Tile::is_valid_for excludes manzu 2–8 in three-player mode.
  if (seatCount === 3 && Math.floor(id / 4) >= 1 && Math.floor(id / 4) <= 7) return null;
  return id;
}

function tileArray(value: unknown, seatCount: number, maxLength = MAX_ARRAY_LENGTH): number[] | null {
  if (!Array.isArray(value) || value.length > maxLength) return null;
  const tiles: number[] = [];
  for (const entry of value) {
    const tile = validTile(entry, seatCount);
    if (tile === null) return null;
    tiles.push(tile);
  }
  return tiles;
}

function scoreLimit(value: unknown): ScoreLimit | undefined | null {
  if (value === undefined || value === null) return undefined;
  return value === "Mangan"
    || value === "Haneman"
    || value === "Baiman"
    || value === "Sanbaiman"
    || value === "CountedYakuman"
    ? value
    : null;
}

function decodeEvaluation(value: unknown): WinEvaluation | null {
  const object = record(value);
  if (!object) return null;
  const keys = Object.keys(object).filter((key) => key === "Ordinary" || key === "Yakuman");
  if (keys.length !== 1 || keys.length !== Object.keys(object).length) return null;
  if (keys[0] === "Ordinary") {
    const ordinary = record(object.Ordinary);
    if (!ordinary) return null;
    const han = integer(ordinary.han, 0, MAX_U32);
    const fu = integer(ordinary.fu, 1, MAX_U32);
    const limit = scoreLimit(ordinary.limit);
    if (han === null || fu === null || limit === null) return null;
    return { Ordinary: { han, fu, ...(limit === undefined ? {} : { limit }) } };
  }
  const yakuman = record(object.Yakuman);
  if (!yakuman) return null;
  const multiplier = integer(yakuman.multiplier, 1, MAX_U32);
  return multiplier === null ? null : { Yakuman: { multiplier } };
}

function decodeYaku(value: unknown, evaluation: WinEvaluation): YakuValue[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > MAX_ARRAY_LENGTH) return null;
  const result: YakuValue[] = [];
  let total = 0;
  for (const entry of value) {
    const object = record(entry);
    if (!object) return null;
    const id = integer(object.id, 0, MAX_U32);
    const name = boundedString(object.name, MAX_NAME_LENGTH);
    if (id === null || name === null) return null;
    const han = object.han === undefined || object.han === null
      ? undefined
      : integer(object.han, 1, MAX_U32);
    const yakumanMultiplier = object.yakuman_multiplier === undefined || object.yakuman_multiplier === null
      ? undefined
      : integer(object.yakuman_multiplier, 1, MAX_U32);
    if (han === null || yakumanMultiplier === null
      || (han !== undefined && yakumanMultiplier !== undefined)) return null;
    if ("Ordinary" in evaluation) {
      if (han === undefined || yakumanMultiplier !== undefined) return null;
      total += han;
      if (!Number.isSafeInteger(total)) return null;
      result.push({ id, name, han });
    } else {
      if (yakumanMultiplier === undefined || han !== undefined) return null;
      total += yakumanMultiplier * 13;
      if (!Number.isSafeInteger(total)) return null;
      result.push({ id, name, yakuman_multiplier: yakumanMultiplier });
    }
  }
  const expected = "Ordinary" in evaluation
    ? evaluation.Ordinary.han
    : evaluation.Yakuman.multiplier * 13;
  return total > 0 && total === expected ? result : null;
}

function decodeMelds(value: unknown, seatCount: number): ResultMeld[] | null {
  if (!Array.isArray(value) || value.length > MAX_ARRAY_LENGTH) return null;
  const melds: ResultMeld[] = [];
  for (const entry of value) {
    const object = record(entry);
    if (!object) return null;
    const kind = object.kind;
    if (kind !== "Chi" && kind !== "Pon" && kind !== "Daiminkan" && kind !== "Ankan" && kind !== "Kakan") return null;
    const tiles = tileArray(object.tiles, seatCount, 32);
    if (!tiles || tiles.length === 0) return null;
    if (typeof object.opened !== "boolean") return null;
    const fromWho = object.from_who === undefined || object.from_who === null
      ? undefined
      : integer(object.from_who, 0, seatCount - 1);
    const calledTile = object.called_tile === undefined || object.called_tile === null
      ? undefined
      : validTile(object.called_tile, seatCount);
    if (fromWho === null || calledTile === null) return null;
    melds.push({
      kind,
      tiles,
      opened: object.opened,
      ...(fromWho === undefined ? {} : { from_who: fromWho }),
      ...(calledTile === undefined ? {} : { called_tile: calledTile }),
    });
  }
  return melds;
}

export function decodeWinResult(value: unknown, mode: string): WinResult | null {
  const seatCount = modeSeatCount(mode);
  const object = record(value);
  if (seatCount === null || !object) return null;
  const eventIndex = integer(object.event_index, 0, MAX_EVENT_INDEX);
  const winner = participantId(object.winner);
  const seat = integer(object.seat, 0, seatCount - 1);
  const target = integer(object.target, 0, seatCount - 1);
  const concealed = tileArray(object.concealed, seatCount, 64);
  const melds = decodeMelds(object.melds, seatCount);
  const winningTile = validTile(object.winning_tile, seatCount);
  const winningTileIndex = integer(object.winning_tile_index, 0, 63);
  const evaluation = decodeEvaluation(object.evaluation);
  const winPoints = integer(object.win_points, 1, MAX_U32);
  if (eventIndex === null || winner === null || seat === null || target === null
    || typeof object.tsumo !== "boolean" || !concealed || !melds || winningTile === null
    || winningTileIndex === null || !evaluation || winPoints === null
    || concealed[winningTileIndex] !== winningTile
    || object.tsumo !== (seat === target)) return null;
  const yaku = decodeYaku(object.yaku, evaluation);
  if (!yaku) return null;
  return {
    event_index: eventIndex,
    winner,
    seat,
    target,
    tsumo: object.tsumo,
    concealed,
    melds,
    winning_tile: winningTile,
    winning_tile_index: winningTileIndex,
    yaku,
    evaluation,
    win_points: winPoints,
  };
}

export function decodeSettlement(value: unknown, mode: string): KyokuSettlement | null {
  const seatCount = modeSeatCount(mode);
  if (seatCount === null) return null;
  const object = record(value);
  if (!object || !Array.isArray(object.changes) || object.changes.length !== seatCount) return null;
  const seen = new Set<string>();
  const changes: ScoreChange[] = [];
  for (let index = 0; index < object.changes.length; index += 1) {
    const change = record(object.changes[index]);
    if (!change) return null;
    const participant = participantId(change.participant_id);
    const seat = integer(change.seat, 0, seatCount - 1);
    const before = integer(change.before, MIN_I32, MAX_I32);
    const delta = integer(change.delta, MIN_I32, MAX_I32);
    const after = integer(change.after, MIN_I32, MAX_I32);
    if (participant === null || seen.has(participant) || seat !== index || before === null || delta === null || after === null) return null;
    if (before + delta !== after) return null;
    seen.add(participant);
    changes.push({ participant_id: participant, seat, before, delta, after });
  }
  const potBefore = integer(object.pot_before, 0, MAX_U32);
  const potAfter = integer(object.pot_after, 0, MAX_U32);
  if (potBefore === null || potAfter === null) return null;
  const drawReason = object.draw_reason === undefined || object.draw_reason === null
    ? undefined
    : boundedString(object.draw_reason, MAX_NAME_LENGTH);
  if (drawReason === null) return null;
  return {
    changes,
    pot_before: potBefore,
    pot_after: potAfter,
    ...(drawReason === undefined ? {} : { draw_reason: drawReason }),
  };
}

function decodeStepId(value: unknown): ResultStepId | null {
  const object = record(value);
  if (!object) return null;
  const matchId = boundedString(object.match_id, MAX_ID_LENGTH);
  const kyokuSequence = integer(object.kyoku_sequence, 1, MAX_EVENT_INDEX);
  const stepIndex = integer(object.step_index, 0, 65_535);
  return matchId === null || kyokuSequence === null || stepIndex === null
    ? null
    : { match_id: matchId, kyoku_sequence: kyokuSequence, step_index: stepIndex };
}

function ids(value: unknown, max: number): string[] | null {
  if (!Array.isArray(value) || value.length > max) return null;
  const result: string[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    const id = participantId(entry);
    if (id === null || seen.has(id)) return null;
    seen.add(id);
    result.push(id);
  }
  return result;
}

export function decodeKyokuEndStep(value: unknown, mode: string): KyokuEndStep | null {
  const seatCount = modeSeatCount(mode);
  const object = record(value);
  if (seatCount === null || !object) return null;
  const id = decodeStepId(object.id);
  const publishedAt = integer(object.published_at_ms, 0, Number.MAX_SAFE_INTEGER);
  const deadlineAt = integer(object.deadline_at_ms, 0, Number.MAX_SAFE_INTEGER);
  const remaining = integer(object.remaining_ms, 0, 5_000);
  const required = ids(object.required, seatCount);
  const confirmed = ids(object.confirmed, seatCount);
  if (!id || publishedAt === null || deadlineAt === null || remaining === null || !required || !confirmed
    || deadlineAt < publishedAt || deadlineAt - publishedAt > 5_000 || remaining > deadlineAt - publishedAt) return null;
  const content = record(object.content);
  if (!content || Object.keys(content).length !== 1) return null;
  let decodedContent: KyokuEndContent | null = null;
  if ("Win" in content) {
    const win = decodeWinResult(content.Win, mode);
    if (win) decodedContent = { Win: win };
  } else if ("Settlement" in content) {
    const settlement = decodeSettlement(content.Settlement, mode);
    if (settlement) decodedContent = { Settlement: settlement };
  } else if ("Draw" in content) {
    const settlement = decodeSettlement(content.Draw, mode);
    if (settlement) decodedContent = { Draw: settlement };
  }
  return decodedContent === null
    ? null
    : {
      id,
      content: decodedContent,
      published_at_ms: publishedAt,
      deadline_at_ms: deadlineAt,
      remaining_ms: remaining,
      required,
      confirmed,
    };
}

export function decodeFinalStandings(value: unknown, mode: string): FinalStandings | null {
  const seatCount = modeSeatCount(mode);
  const object = record(value);
  const wireMode = ({
    "4p-red-east": "FourPlayerRedEast", "4p-red-half": "FourPlayerRedHalf",
    "3p-red-east": "ThreePlayerRedEast", "3p-red-half": "ThreePlayerRedHalf",
  } as Record<string, string>)[mode] ?? mode;
  if (seatCount === null || !object || object.mode !== wireMode) return null;
  if (!Array.isArray(object.players) || object.players.length !== seatCount
    || !Array.isArray(object.final_scores) || object.final_scores.length !== seatCount) return null;
  const finalScores: number[] = [];
  for (const score of object.final_scores) {
    const parsed = integer(score, MIN_I32, MAX_I32);
    if (parsed === null) return null;
    finalScores.push(parsed);
  }
  const idsSeen = new Set<string>();
  const ranksSeen = new Set<number>();
  const players: MatchPlayerResult[] = [];
  for (let index = 0; index < object.players.length; index += 1) {
    const player = record(object.players[index]);
    if (!player) return null;
    const participant = participantId(player.participant_id);
    const displayName = boundedString(player.display_name, MAX_NAME_LENGTH);
    const kind = player.kind;
    const seat = integer(player.seat, 0, seatCount - 1);
    const score = integer(player.final_score, MIN_I32, MAX_I32);
    const rank = integer(player.rank, 1, seatCount);
    if (participant === null || displayName === null
      || (kind !== "Human" && kind !== "MJAI" && kind !== "MCP" && kind !== "BuiltInBot")
      || seat !== index || score === null || rank === null
      || score !== finalScores[index] || idsSeen.has(participant) || ranksSeen.has(rank)) return null;
    idsSeen.add(participant);
    ranksSeen.add(rank);
    const expectedRank = 1 + finalScores.filter((other, otherSeat) =>
      other > score || (other === score && otherSeat < seat),
    ).length;
    if (rank !== expectedRank) return null;
    players.push({ participant_id: participant, display_name: displayName, kind, seat, final_score: score, rank });
  }
  return { mode: object.mode, players, final_scores: finalScores };
}

export function resultStepKey(id: ResultStepId): string {
  return `${id.match_id}:${id.kyoku_sequence}:${id.step_index}`;
}
