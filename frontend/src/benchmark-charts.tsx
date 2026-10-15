import { useEffect, useId, useState } from "react";
import { Bar, BarChart, CartesianGrid, Line, LineChart, ReferenceLine, XAxis, YAxis } from "recharts";
import type { ParticipantAnalytics, RunAnalytics } from "./benchmark-analytics";
import { ChartContainer, ChartTooltip, ChartTooltipContent, ChartLegend, ChartLegendContent, type ChartConfig } from "./components/ui/chart";
import { Card, CardHeader, CardTitle, CardDescription, CardContent } from "./components/ui/card";
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from "./components/ui/accordion";
import { Button } from "./components/ui/button";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "./components/ui/table";
import { Alert, AlertTitle, AlertDescription } from "./components/ui/alert";

type Ready = Extract<RunAnalytics, { kind: "ready" }>;
type Family = "cumulative" | "movingRank" | "distribution" | "net";
const titles: Record<Family, string> = { cumulative: "Cumulative net score", movingRank: "Moving-average rank", distribution: "Rank distribution", net: "Per-Match net score" };
const descriptions: Record<Family, string> = {
  cumulative: "Absolute full-run total relative to the table mean; Match sequence × net points.",
  movingRank: "Trailing 10 completed Matches (or fewer); Match sequence × mean rank, 1 is best.",
  distribution: "Horizontal 100% stacked ranks; percentage of completed Matches, not individual Match links.",
  net: "Signed final points minus the Match table mean; Match sequence × net points.",
};
const dashes = [undefined, "8 3", "3 3", "10 3 2 3"];
const compareId = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
function ChartCard({ family, analytics, participants, onOpenMatch }: { family: Family; analytics: Ready; participants: ParticipantAnalytics[]; onOpenMatch(id: string): void }) {
  const id = useId().replace(/:/g, "");
  const title = titles[family];
  const [windowSize, setWindowSize] = useState("all");
  const [hidden, setHidden] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const leader = [...participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity) || compareId(a.participantId, b.participantId))[0];
  useEffect(() => {
    if (selected === null && analytics.completed > 0 && leader) setSelected(leader.participantId);
  }, [analytics.completed, leader, selected]);
  const chosen = participants.find(p => p.participantId === selected);
  const start = windowSize === "50" ? Math.max(0, analytics.completed - 50) : 0;
  const n = analytics.completed - start;
  const config: ChartConfig = Object.fromEntries(participants.map((p, i) => [`p${i}`, { label: `${p.displayName} (${p.participantId})`, color: `var(--chart-${i + 1})` }]));
  const rankCount = participants.length;
  const ranks: ChartConfig = Object.fromEntries(Array.from({ length: rankCount }, (_, i) => [`r${i}`, { label: `Rank ${i + 1}`, color: `color-mix(in oklch, var(--foreground) ${85 - i * 15}%, var(--background))` }]));
  const data = (participants[0]?.points.slice(start) ?? []).map((point, index) => ({ sequence: point.sequence, matchId: point.matchId,
    ...Object.fromEntries(participants.map((p, i) => [`p${i}`, p.points[start + index][family === "movingRank" ? "movingRank" : family === "net" ? "net" : "cumulative"]])),
  }));
  const distribution = participants.filter(p => !hidden.includes(p.participantId)).map(p => ({
    name: `${p.displayName} (${p.participantId})`, ...Object.fromEntries(p.rankCounts.map((count, i) => [`r${i}`, analytics.completed ? count / analytics.completed * 100 : 0])),
  }));
  const details = (state: { activeIndex?: number | string | null }) => {
    const point = state.activeIndex == null ? undefined : data[Number(state.activeIndex)];
    if (point) onOpenMatch(point.matchId);
  };
  const tooltip = <ChartTooltip content={<ChartTooltipContent labelFormatter={(_, payload) => family === "distribution" ? payload[0]?.payload?.name : `Match ${payload[0]?.payload?.sequence}`} />} />;
  const legend = <ChartLegend content={<ChartLegendContent className="flex-wrap" />} />;
  return <Card className={family === "cumulative" ? "benchmark-chart-card benchmark-hero" : "benchmark-chart-card"}>
    <CardHeader><CardTitle><h3 id={`${id}-title`}>{title}</h3></CardTitle><CardDescription>{descriptions[family]}</CardDescription></CardHeader>
    <CardContent><section role="region" aria-labelledby={`${id}-title`}>
      <p>{family === "distribution" ? `All completed Matches: n = ${analytics.completed}; denominator per Participant.` : `Sample n = ${n} of ${analytics.completed} completed Matches.`}</p>
      {family !== "distribution" && <label>{title} window <select aria-label={`${title} window`} value={windowSize} onChange={e => setWindowSize(e.target.value)}><option value="all">Full Run</option><option value="50">Recent 50</option></select></label>}
      {family === "net" ? <label>Net score Participant <select aria-label="Net score Participant" value={selected ?? ""} onChange={e => setSelected(e.target.value)}><option value="" disabled>Waiting for standings</option>{participants.map(p => <option key={p.participantId} value={p.participantId}>{p.displayName} ({p.participantId})</option>)}</select></label> : <fieldset className="benchmark-series-controls"><legend>Visible Participants — {title}</legend>{participants.map(p => <label key={p.participantId}><input type="checkbox" aria-label={`${title} ${p.displayName} (${p.participantId})`} checked={!hidden.includes(p.participantId)} onChange={e => setHidden(e.target.checked ? hidden.filter(id => id !== p.participantId) : [...hidden, p.participantId])} />{p.displayName} ({p.participantId})</label>)}</fieldset>}
      {family === "cumulative" && windowSize === "50" && <p>Window net: {participants.map(p => `${p.displayName} (${p.participantId}) ${(p.points.slice(start).reduce((sum, point) => sum + point.net, 0)).toLocaleString()}`).join("; ")}. Cumulative lines retain full-run totals.</p>}
      <ChartContainer config={family === "distribution" ? ranks : config} className="benchmark-chart-region" aria-label={title} onClickCapture={event => event.currentTarget.querySelector<SVGSVGElement>(".recharts-surface")?.focus({ preventScroll: true })}>
        {family === "distribution" ? <BarChart accessibilityLayer layout="vertical" data={distribution} margin={{ left: 8, right: 24 }}>
          <defs>{Array.from({ length: rankCount }, (_, i) => <pattern key={i} id={`${id}-rank-${i}`} width={5 + i * 2} height={5 + i * 2} patternUnits="userSpaceOnUse"><rect width="100%" height="100%" fill={`var(--color-r${i})`} /><path d={`M0 0L${5 + i * 2} ${5 + i * 2}`} stroke="var(--background)" strokeWidth="1" /></pattern>)}</defs>
          <XAxis type="number" domain={[0, 100]} unit="%" /><YAxis type="category" dataKey="name" width={100} />{tooltip}{legend}
          {Array.from({ length: rankCount }, (_, i) => <Bar key={i} dataKey={`r${i}`} stackId="ranks" fill={`url(#${id}-rank-${i})`} isAnimationActive={false} />)}
        </BarChart> : family === "net" ? <BarChart accessibilityLayer data={data}>
          <CartesianGrid vertical={false} /><XAxis dataKey="sequence" /><YAxis /><ReferenceLine y={0} stroke="var(--foreground)" />{tooltip}{legend}
          {chosen && <Bar dataKey={`p${participants.indexOf(chosen)}`} fill={`var(--color-p${participants.indexOf(chosen)})`} isAnimationActive={false} onClick={(_, index) => { const point = data[index]; if (point) onOpenMatch(point.matchId); }} />}
        </BarChart> : <LineChart accessibilityLayer data={data} onClick={details}>
          <CartesianGrid vertical={false} /><XAxis dataKey="sequence" /><YAxis reversed={family === "movingRank"} domain={family === "movingRank" ? [1, rankCount] : ["auto", "auto"]} allowDecimals={family !== "movingRank"} />
          {family === "cumulative" && <ReferenceLine y={0} ifOverflow="extendDomain" stroke="var(--foreground)" />}{tooltip}{legend}
          {participants.map((p, i) => !hidden.includes(p.participantId) && <Line key={p.participantId} type="linear" dataKey={`p${i}`} stroke={`var(--color-p${i})`} strokeDasharray={dashes[i]} dot={false} isAnimationActive={false} />)}
        </LineChart>}
      </ChartContainer>
      {family === "movingRank" && <p>Each point uses n = min(10, Match sequence). Exact n is in numeric results.</p>}
    </section></CardContent>
  </Card>;
}

export function BenchmarkCharts({ analytics, onOpenMatch }: { analytics: Ready; onOpenMatch(matchId: string): void }) {
  const participants = [...analytics.participants].sort((a, b) => compareId(a.participantId, b.participantId));
  const [mobileSections, setMobileSections] = useState(["performance", "consistency"]);
  const [desktop, setDesktop] = useState(() => window.matchMedia("(min-width: 1024px)").matches);
  useEffect(() => {
    const query = window.matchMedia("(min-width: 1024px)");
    const update = () => setDesktop(query.matches);
    query.addEventListener("change", update);
    update();
    return () => query.removeEventListener("change", update);
  }, []);
  const [page, setPage] = useState(0);
  const [sequence, setSequence] = useState(1);
  const points = participants[0]?.points ?? [];
  const lastPage = Math.max(0, Math.ceil(points.length / 25) - 1);
  const currentPage = Math.min(page, lastPage);
  const rows = [...points].reverse().slice(currentPage * 25, currentPage * 25 + 25);
  const selected = points.find(point => point.sequence === sequence);
  return <section className="benchmark-dashboard" aria-label="Benchmark analytics">
    {analytics.provisional && <Alert><AlertTitle>Provisional results</AlertTitle><AlertDescription>Sample n = {analytics.completed} completed Matches. Small samples do not establish playing strength.</AlertDescription></Alert>}
    <nav aria-label="Analytics jump menu"><a href="#benchmark-performance">Performance</a> · <a href="#benchmark-consistency">Consistency</a> · <a href="#benchmark-numeric">Match history</a></nav>
    <h2>Performance and consistency</h2>
    <Accordion multiple value={desktop ? ["performance", "consistency"] : mobileSections} onValueChange={value => { if (!desktop) setMobileSections(value); }} className="benchmark-chart-groups">
      <AccordionItem value="performance" id="benchmark-performance"><AccordionTrigger>Performance</AccordionTrigger><AccordionContent><div className="benchmark-chart-grid">
        <ChartCard family="cumulative" analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} />
        <ChartCard family="net" analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} />
      </div></AccordionContent></AccordionItem>
      <AccordionItem value="consistency" id="benchmark-consistency"><AccordionTrigger>Consistency</AccordionTrigger><AccordionContent><div className="benchmark-chart-grid">
        <ChartCard family="movingRank" analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} />
        <ChartCard family="distribution" analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} />
      </div></AccordionContent></AccordionItem>
    </Accordion>
    <h2>Participant statistics</h2>
    <Table aria-label="Participant statistics" tabIndex={0}><TableHeader><TableRow><TableHead>Participant</TableHead><TableHead>Mean rank</TableHead><TableHead>First-place rate</TableHead><TableHead>Net score</TableHead>{participants.map((_, i) => <TableHead key={i}>Rank {i + 1} count / rate</TableHead>)}</TableRow></TableHeader><TableBody>
      {[...participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity) || compareId(a.participantId, b.participantId)).map(p => <TableRow key={p.participantId}><TableCell>{p.displayName} ({p.participantId})</TableCell><TableCell>{p.averageRank?.toFixed(2) ?? "—"}</TableCell><TableCell>{p.firstPlaceRate === null ? "—" : `${(p.firstPlaceRate * 100).toFixed(1)}%`}</TableCell><TableCell>{p.cumulativeNet.toLocaleString()}</TableCell>{p.rankCounts.map((count, i) => <TableCell key={i}>{count} / {analytics.completed ? (count / analytics.completed * 100).toFixed(1) : "0.0"}%</TableCell>)}</TableRow>)}
    </TableBody></Table>
    <section id="benchmark-numeric"><h2>Match history and numeric chart equivalent</h2>
      <label>Selected Match sequence <input aria-label="Selected Match sequence" type="number" min={1} max={Math.max(1, points.length)} value={sequence} onChange={e => setSequence(Number(e.target.value))} /></label>
      <Button variant="outline" disabled={!selected} onClick={() => { if (selected) onOpenMatch(selected.matchId); }}>View Match details</Button>
      <p>Recent-first, 25 Matches per page. Net / full-run cumulative / trailing mean rank (sample n).</p>
      <Table aria-label="Per-Match numeric results"><TableHeader><TableRow><TableHead>Match</TableHead>{participants.map(p => <TableHead key={p.participantId}>{p.displayName} ({p.participantId})</TableHead>)}</TableRow></TableHeader><TableBody>{rows.map(point => <TableRow key={point.matchId}><TableCell><Button variant="outline" onClick={() => onOpenMatch(point.matchId)}>View Match {point.sequence} details</Button></TableCell>{participants.map(p => {
        const value = p.points[point.sequence - 1];
        return <TableCell key={p.participantId}>{value.net.toLocaleString()} / {value.cumulative.toLocaleString()} / {value.movingRank.toFixed(2)} (n={value.movingN})</TableCell>;
      })}</TableRow>)}</TableBody></Table>
      <div className="action-row"><Button variant="outline" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>Newer Matches</Button><span>Page {currentPage + 1} of {lastPage + 1}</span><Button variant="outline" disabled={currentPage >= lastPage} onClick={() => setPage(currentPage + 1)}>Older Matches</Button></div>
    </section>
  </section>;
}
