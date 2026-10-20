import type { BenchmarkRun } from "./api";

export type AnalyticsPoint = { sequence: number; matchId: string; net: number; cumulative: number; movingRank: number; movingN: number };
export type ParticipantAnalytics = {
  participantId: string; displayName: string; averageRank: number | null; firstPlaceRate: number | null;
  cumulativeNet: number; rankCounts: number[]; points: AnalyticsPoint[];
};
export type RunAnalytics = { kind: "ready"; completed: number; provisional: boolean; participants: ParticipantAnalytics[] }
  | { kind: "invalid"; reason: string };

export function deriveRunAnalytics(input: Pick<BenchmarkRun, "game_mode" | "completed" | "roster" | "matches">): RunAnalytics {
  if (!input || !["3p-red-east", "3p-red-half", "4p-red-east", "4p-red-half"].includes(input.game_mode)) {
    return { kind: "invalid", reason: "Unsupported game mode." };
  }
  const playerCount = input.game_mode.startsWith("3p") ? 3 : 4;
  if (!Number.isInteger(input.completed) || input.completed < 0 || input.completed > 1000
    || !Array.isArray(input.matches) || input.matches.length !== input.completed) {
    return { kind: "invalid", reason: "Completed Match count does not match persisted records." };
  }
  if (!Array.isArray(input.roster) || input.roster.length !== playerCount) {
    return { kind: "invalid", reason: "Roster does not match the game mode." };
  }
  const participantIds = new Set<string>();
  const initialSeats = new Set<number>();
  for (const participant of input.roster) {
    if (!participant || typeof participant.participant_id !== "string" || !participant.participant_id
      || participantIds.has(participant.participant_id) || typeof participant.display_name !== "string"
      || !Number.isInteger(participant.initial_seat) || participant.initial_seat < 0
      || participant.initial_seat >= playerCount || initialSeats.has(participant.initial_seat)) {
      return { kind: "invalid", reason: "Roster requires unique Participant IDs and valid initial Seats." };
    }
    participantIds.add(participant.participant_id);
    initialSeats.add(participant.initial_seat);
  }
  if (input.matches.some(match => !match || !Number.isInteger(match.sequence))) {
    return { kind: "invalid", reason: "Invalid Match sequence." };
  }
  const matches = [...input.matches].sort((a, b) => a.sequence - b.sequence);
  const matchIds = new Set<string>();
  for (const [index, match] of matches.entries()) {
    if (match.sequence !== index + 1 || typeof match.match_id !== "string" || !match.match_id
      || matchIds.has(match.match_id) || !Array.isArray(match.results) || match.results.length !== playerCount) {
      return { kind: "invalid", reason: "Matches require unique IDs, contiguous sequences and complete results." };
    }
    matchIds.add(match.match_id);
    const resultIds = new Set<string>();
    const seats = new Set<number>();
    for (const result of match.results) {
      if (!result || !participantIds.has(result.participant_id) || resultIds.has(result.participant_id)
        || !Number.isInteger(result.seat) || result.seat < 0 || result.seat >= playerCount || seats.has(result.seat)
        || !Number.isInteger(result.rank) || result.rank < 1 || result.rank > playerCount
        || !Number.isInteger(result.final_score)) {
        return { kind: "invalid", reason: "Results require roster Participants, unique Seats and valid integer scores and ranks." };
      }
      resultIds.add(result.participant_id);
      seats.add(result.seat);
    }
    if (!Number.isFinite(match.results.reduce((sum, result) => sum + result.final_score, 0))) {
      return { kind: "invalid", reason: "Table score total is not finite." };
    }
  }
  // Keep roster identity order; consumers sort a copy for a leaderboard or chart legend.
  const participants: ParticipantAnalytics[] = input.roster.map(participant => {
    let cumulativeNet = 0;
    let rankSum = 0;
    const ranks: number[] = [];
    const rankCounts = Array<number>(playerCount).fill(0);
    const points = matches.map(match => {
      const result = match.results.find(result => result.participant_id === participant.participant_id)!;
      const mean = match.results.reduce((sum, result) => sum + result.final_score, 0) / playerCount;
      const net = result.final_score - mean;
      cumulativeNet += net;
      rankSum += result.rank;
      rankCounts[result.rank - 1]++;
      ranks.push(result.rank);
      const trailing = ranks.slice(-10);
      return { sequence: match.sequence, matchId: match.match_id, net, cumulative: cumulativeNet,
        movingRank: trailing.reduce((sum, rank) => sum + rank, 0) / trailing.length, movingN: trailing.length };
    });
    return { participantId: participant.participant_id, displayName: participant.display_name,
      averageRank: input.completed ? rankSum / input.completed : null,
      firstPlaceRate: input.completed ? rankCounts[0] / input.completed : null,
      cumulativeNet, rankCounts, points };
  });
  if (participants.some(participant => participant.points.some(point => !Number.isFinite(point.net) || !Number.isFinite(point.cumulative)))) {
    return { kind: "invalid", reason: "Derived scores are not finite." };
  }
  return { kind: "ready", completed: input.completed, provisional: input.completed < 10, participants };
}
