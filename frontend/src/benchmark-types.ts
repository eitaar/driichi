import type { BenchmarkRun } from "./api";

export type ReplayAvailability = "unknown" | "available" | "unavailable" | "too_large";
export type BenchmarkMatch = BenchmarkRun["matches"][number];
export type BenchmarkRoster = BenchmarkRun["roster"];
export type BenchmarkMatchEvidence = BenchmarkMatch & { replay_availability: ReplayAvailability };
