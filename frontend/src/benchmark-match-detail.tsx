import { useEffect, useRef } from "react";
import type { BenchmarkMatchEvidence, BenchmarkRoster } from "./benchmark-types";
import { useGameplayViewportSupport } from "./game/viewport";
import { navigate } from "./routes";
import { Button } from "./components/ui/button";
import { Sheet, SheetContent, SheetHeader, SheetTitle, SheetDescription } from "./components/ui/sheet";
import { Drawer, DrawerContent, DrawerHeader, DrawerTitle, DrawerDescription } from "./components/ui/drawer";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "./components/ui/table";

export function BenchmarkMatchDetail({ match, roster, onClose, onRetryReplay }: {
  match: BenchmarkMatchEvidence | null; roster: BenchmarkRoster; onClose(): void; onRetryReplay?: () => void;
}) {
  const desktop = useGameplayViewportSupport();
  const trigger = useRef<HTMLElement | SVGElement | null>(null);
  const open = match !== null;
  // Remember the opener only. Restoring is owned by the dialog's `finalFocus`, so a single
  // mechanism moves focus back; a second restore can land after the user re-opens a control
  // (for example the chart's Select) and close it again.
  useEffect(() => {
    if (!open) return;
    const active = document.activeElement;
    trigger.current = active instanceof HTMLElement || active instanceof SVGElement ? active : null;
  }, [open]);
  const restoreFocus = () => { trigger.current?.focus({ preventScroll: true }); return false; };
  const status = match?.replay_availability;
  const body = match && <div className="flex min-h-0 flex-col gap-4 overflow-auto overscroll-contain p-4 [overflow-wrap:anywhere]">
    <p className="break-all tabular-nums">{match.match_id}</p>
    <Table aria-label="Match placements" tabIndex={0}><TableHeader><TableRow><TableHead>Participant</TableHead><TableHead>Seat</TableHead><TableHead>Rank</TableHead><TableHead>Final points</TableHead></TableRow></TableHeader><TableBody>
      {[...match.results].sort((a, b) => a.rank - b.rank || a.seat - b.seat).map(result => <TableRow key={result.participant_id}>
        <TableCell>{roster.find(p => p.participant_id === result.participant_id)?.display_name}</TableCell><TableCell>{result.seat + 1}</TableCell><TableCell>{result.rank}</TableCell><TableCell>{result.final_score.toLocaleString()}</TableCell>
      </TableRow>)}
    </TableBody></Table>
    <p role="status">{status === "available" ? "Replay available." : status === "unavailable" ? "Replay unavailable. The recorded placements and points remain valid." : status === "too_large" ? "Replay is too large for the viewer. Placements and points remain available." : "Replay status not verified. Placements and points remain available; missing or failed metadata never establishes availability."}</p>
    {status === "available" && <a href={`/admin/replays/${encodeURIComponent(match.match_id)}`} onClick={event => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      event.preventDefault(); navigate(`/admin/replays/${encodeURIComponent(match.match_id)}`);
    }}>Open Replay</a>}
    {status === "unknown" && onRetryReplay && <Button variant="outline" onClick={onRetryReplay}>Retry Replay status</Button>}
  </div>;
  const close = <Button variant="outline" onClick={onClose}>Close</Button>;
  return desktop ? <Sheet open={open} onOpenChange={value => { if (!value) onClose(); }}>
    <SheetContent showCloseButton={false} finalFocus={restoreFocus}>
      <SheetHeader><SheetTitle>Match {match?.sequence} details</SheetTitle><SheetDescription>Persisted completed Match results.</SheetDescription>{close}</SheetHeader>{body}
    </SheetContent>
  </Sheet> : <Drawer open={open} onOpenChange={value => { if (!value) onClose(); }}>
    <DrawerContent finalFocus={restoreFocus}>
      <DrawerHeader><DrawerTitle>Match {match?.sequence} details</DrawerTitle><DrawerDescription>Persisted completed Match results.</DrawerDescription>{close}</DrawerHeader>{body}
    </DrawerContent>
  </Drawer>;
}
