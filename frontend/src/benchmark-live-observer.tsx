import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, ApiProblem, problemFrom } from "./api";
import { ThreeTable } from "./game/three-table";
import { tileLabel } from "./game/tiles";

export function BenchmarkLiveObserver({ code }: { code: string }) {
  const [open, setOpen] = useState(false);
  const live = useQuery({ queryKey: ["admin", "benchmark", "live", code], queryFn: () => api.getBenchmarkLive(code), enabled: open,
    refetchInterval: query => query.state.error instanceof ApiProblem && query.state.error.status === 401 ? false : 1000,
    refetchIntervalInBackground: false,
  });
  const projection = live.data?.projection;
  return <details className="admin-disclosure" onToggle={event => setOpen(event.currentTarget.open)}><summary>Live table — all current hands</summary>
    {open && (live.isError ? <div><p className="form-error" role="alert">{problemFrom(live.error).detail}</p><button className="button button-secondary" onClick={() => { void live.refetch(); }}>Retry</button></div> : !live.data || !live.isFetchedAfterMount ? <p role="status">Connecting to live table…</p> : projection ? <>
      <p className="field-hint">Read-only Admin view · Revision {live.data.revision} · Updates every second</p>
      <div className="benchmark-live replay-table-wrap"><ThreeTable projection={projection} room={null} surface="replay" reducedMotion /></div>
      <details><summary>Current hands as text</summary><ul>{projection.players?.map(player => <li key={player.participant_id}>{player.display_name} ({player.participant_id}): {player.hand?.map(tileLabel).join(", ")}</li>)}</ul></details>
    </> : <p>No active Match.</p>)}
  </details>;
}
