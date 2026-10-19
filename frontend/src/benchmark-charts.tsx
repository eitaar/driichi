import { useEffect, useId, useState, useRef, type ComponentProps } from "react";
import {radarUpperBound, fitRadar} from "./benchmark-chart-geometry";
import { LabelList, Symbols, RadarChart, Radar, PolarGrid, PolarAngleAxis, PolarRadiusAxis, Bar, BarChart, CartesianGrid, Line, LineChart, ReferenceLine, XAxis, YAxis } from "recharts";
import type { ParticipantAnalytics, RunAnalytics } from "./benchmark-analytics";
import { ChartContainer, ChartTooltip, ChartTooltipContent, ChartLegend, ChartLegendContent, type ChartConfig } from "./components/ui/chart";
import { Card, CardHeader, CardTitle, CardContent } from "./components/ui/card";
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from "./components/ui/accordion";
import { Button } from "./components/ui/button";
import { Table, TableHeader, TableHead, TableBody, TableRow, TableCell } from "./components/ui/table";
import { Alert, AlertTitle, AlertDescription } from "./components/ui/alert";
import { Select, SelectTrigger, SelectValue, SelectContent, SelectGroup, SelectItem } from "./components/ui/select";
import { Checkbox } from "./components/ui/checkbox";
import { Field, FieldGroup, FieldSet, FieldLegend, FieldLabel } from "./components/ui/field";
import { Input } from "./components/ui/input";

type Ready = Extract<RunAnalytics, { kind: "ready" }>;
type Family = "cumulative" | "movingRank" | "distribution" | "net";
const titles: Record<Family, string> = { cumulative: "Cumulative net score", movingRank: "Moving-average rank", distribution: "Rank distribution", net: "Per-Match net score" };
const descriptions: Record<Family, string> = {
  cumulative: "Absolute full-run total relative to the table mean; Match sequence × net points.",
  movingRank: "Trailing 10 completed Matches (or fewer); Match sequence × mean rank, 1 is best.",
  distribution: "Player axes with Rank percentages of completed Matches; shared full-roster scale, not individual Match links.",
  net: "Signed final points minus the Match table mean; Match sequence × net points.",
};
const markerShapes = ["circle", "square", "diamond", "triangle"] as const;
const compareId = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
function ChartCard({ family, analytics, participants, onOpenMatch }: { family: Family; analytics: Ready; participants: ParticipantAnalytics[]; onOpenMatch(id: string): void }) {
  const id = useId().replace(/:/g, "");
  const title = titles[family];
  const radarCard = useRef<HTMLDivElement>(null);
  const [radarGeometry, setRadarGeometry] = useState({cx: 120, cy: 100, radius: 60});
  const [windowSize, setWindowSize] = useState("50");
  const [hidden, setHidden] = useState<string[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const selectedIndex = participants.findIndex(p => p.participantId === selected);
  const leader = [...participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity) || compareId(a.participantId, b.participantId))[0];
  useEffect(() => {
    if (selectedIndex < 0 && analytics.completed > 0 && leader) setSelected(leader.participantId);
  }, [analytics.completed, leader, selectedIndex]);
  const [cyclingPaused, setCyclingPaused] = useState(false);
  const [readingPointer, setReadingPointer] = useState(false);
  const [readingFocus, setReadingFocus] = useState(false);
  const [pageVisible, setPageVisible] = useState(() => !document.hidden);
  const [manualRevision, setManualRevision] = useState(0);
  useEffect(() => {
    if (family !== "net" || participants.length < 2) return;
    const visibility = () => setPageVisible(!document.hidden);
    document.addEventListener("visibilitychange", visibility);
    visibility();
    return () => document.removeEventListener("visibilitychange", visibility);
  }, [family, participants.length]);
  // Five-second analytics polls replace objects, not the ordered Participant identities owning this timer.
  const cycleRoster = JSON.stringify(participants.map(p => p.participantId));
  const nextParticipantId = participants.length > 1 ? participants[(selectedIndex + 1) % participants.length].participantId : null;
  useEffect(() => {
    if (family !== "net" || selectedIndex < 0 || nextParticipantId === null || cyclingPaused || readingPointer || readingFocus || !pageVisible) return;
    const timer = window.setInterval(() => setSelected(nextParticipantId), 8000);
    return () => window.clearInterval(timer);
  }, [family, cycleRoster, selected, selectedIndex, nextParticipantId, cyclingPaused, readingPointer, readingFocus, pageVisible, manualRevision]);
  const chosen = participants.find(p => p.participantId === selected);
  const start = windowSize === "50" ? Math.max(0, analytics.completed - 50) : 0;
  const n = analytics.completed - start;
  const config: ChartConfig = Object.fromEntries(participants.map((p, i) => [`p${i}`, { label: p.displayName, color: `var(--chart-${i + 1})`, icon: () => <svg aria-hidden="true" className="identity-marker" data-shape={markerShapes[i]} viewBox="0 0 20 20"><Symbols cx={10} cy={10} size={64} type={markerShapes[i]} fill={`var(--color-p${i})`} /></svg> }]));
  const rankCount = participants.length;
  const axisRem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
  const plotMargin = {top: axisRem * 1.5, right: axisRem * 1.5, bottom: axisRem / 2, left: axisRem};
  // shortcut: label only up to three Matches; measure plot spacing if denser value labels are needed.
  const showNetLabels = axisRem <= 16 && innerWidth >= 360 && n <= 3;
  const point = (i: number, active = false) => (props: {cx?: number; cy?: number; index?: number}) => <Symbols key={`${i}-${props.index}`} className={active ? "series-marker active-marker" : "series-marker"} data-shape={markerShapes[i]} data-participant-index={i} cx={props.cx} cy={props.cy} size={active ? 80 : 30} type={markerShapes[i]} fill={`var(--color-p${i})`} stroke="var(--card)" strokeWidth={1} />;
  const rankPoint = (i: number, active = false) => (props: {cx?: number; cy?: number; index?: number}) => <Symbols key={`rank-${i}-${props.index}`} className={active ? "series-marker active-marker" : "series-marker"} data-shape={markerShapes[i]} data-rank-index={i} cx={props.cx} cy={props.cy} size={active ? 80 : 30} type={markerShapes[i]} fill={`var(--color-r${i})`} stroke="var(--card)" strokeWidth={1} />;
  const rankConfig: ChartConfig = Object.fromEntries(Array.from({length: rankCount}, (_, i) => [`r${i}`, {label: `Rank ${i + 1}`, color: `var(--chart-${i + 1})`, icon: () => <svg aria-hidden="true" className="identity-marker" data-shape={markerShapes[i]} viewBox="0 0 20 20"><Symbols cx={10} cy={10} size={64} type={markerShapes[i]} fill={`var(--color-r${i})`} /></svg>}]));

  const data = (participants[0]?.points.slice(start) ?? []).map((point, index) => ({ sequence: point.sequence, matchId: point.matchId,
    ...Object.fromEntries(participants.map((p, i) => [`p${i}`, p.points[start + index][family === "movingRank" ? "movingRank" : family === "net" ? "net" : "cumulative"]])),
  }));
  const distribution = participants.filter(p => !hidden.includes(p.participantId)).map(p => ({name: p.displayName, participantId: p.participantId, ...Object.fromEntries(p.rankCounts.map((count, i) => [`r${i}`, analytics.completed ? count / analytics.completed * 100 : 0]))}));
  const radarMaximum = radarUpperBound(participants, analytics.completed);
  // SAFETY: Recharts 3.8 combineAxisTicks consumes numeric ticks; the polar declaration incorrectly requires computed TickItems.
  const radarTicks = [0, radarMaximum / 2, radarMaximum] as unknown as ComponentProps<typeof PolarRadiusAxis>["ticks"];
  useEffect(() => {
    if (family !== "distribution" || hidden.length === participants.length) return;
    const host = radarCard.current?.querySelector<HTMLElement>(".radar-host");
    if (!host) return;
    let frame = 0, alive = true;
    const measure = () => {
      frame = 0;
      if (!alive) return;
      const legendElement = host.querySelector<HTMLElement>(".house-chart-legend"), labels = host.querySelectorAll<SVGTextElement>(".radar-label-metric");
      // Recharts registers its legend/axis children after this effect's initial commit.
      if (!legendElement || !labels.length) { schedule(); return; }
      observer.observe(legendElement); labels.forEach(label => observer.observe(label));
      const box = host.getBoundingClientRect(), legendBox = legendElement.getBoundingClientRect();
      const metrics = Array.from(labels, label => label.getBBox());
      const width = Math.max(...metrics.map((b, i) => i === 0 ? b.width / 2 : b.width));
      const geometry = fitRadar(box.width, legendBox.top - box.top, width, Math.max(...metrics.map(b => b.height)), parseFloat(getComputedStyle(document.documentElement).fontSize) || 16);
      setRadarGeometry(previous => Math.abs(previous.cx - geometry.cx) >= .5 || Math.abs(previous.cy - geometry.cy) >= .5 || Math.abs(previous.radius - geometry.radius) >= .5 ? geometry : previous);
    };
    const schedule = () => { if (alive && !frame) frame = requestAnimationFrame(measure); };
    const observer = new ResizeObserver(schedule);
    observer.observe(host);
    schedule(); document.fonts?.ready.then(schedule);
    return () => { alive = false; observer.disconnect(); cancelAnimationFrame(frame); };
  }, [family, participants, hidden.length]);
  const details = (state: { activeIndex?: number | string | null }) => {
    const point = state.activeIndex == null ? undefined : data[Number(state.activeIndex)];
    if (point) onOpenMatch(point.matchId);
  };
  const tooltip = <ChartTooltip cursor={false} content={<ChartTooltipContent className={family === "distribution" ? "radar-tooltip" : undefined} formatter={family === "distribution" ? (value, _name, item) => {const rankIndex=Number(String(item.dataKey).slice(1));const participant=participants.find(p => p.participantId === item.payload.participantId);const count=participant?.rankCounts[rankIndex];return <><span className="radar-tooltip-name">{rankConfig[String(item.dataKey)]?.label}</span><span className="radar-tooltip-value">{Number(value).toFixed(1)}% ({count}/{analytics.completed})</span></>;} : undefined} labelFormatter={(_, payload) => family === "distribution" ? payload[0]?.payload?.name : `Match ${payload[0]?.payload?.sequence}`} />} />;
  const legend = <ChartLegend wrapperStyle={family === "distribution" ? {left: 0, width: "100%"} : undefined} verticalAlign="bottom" height={axisRem > 16 ? axisRem * (family === "net" ? 2.5 : 6) : axisRem * (innerWidth >= 1024 ? 3 : 4)} content={<ChartLegendContent className="house-chart-legend" />} />;
  const windowItems = [{ value: "all", label: "Full Run" }, { value: "50", label: "Recent 50" }];
  const participantItems = [{ value: "", label: "Waiting for standings" }, ...participants.map(p => ({ value: p.participantId, label: p.displayName }))];
  return <Card ref={family === "distribution" ? radarCard : undefined} data-chart-family={family} onPointerEnter={family === "net" ? () => setReadingPointer(true) : undefined} onPointerLeave={family === "net" ? () => setReadingPointer(false) : undefined} onFocusCapture={family === "net" ? () => setReadingFocus(true) : undefined} onBlurCapture={family === "net" ? event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setReadingFocus(false); } : undefined} className={family === "cumulative" ? "benchmark-chart-card min-w-0 lg:col-span-2" : "benchmark-chart-card min-w-0"}>
    <CardHeader><CardTitle><h2 id={`${id}-title`}>{title}</h2></CardTitle>{family === "cumulative" && analytics.provisional && <span className="provisional-state" role="status">Provisional</span>}</CardHeader>
    <CardContent tabIndex={0} role="region" aria-label={`${title} chart and options`}><section role="region" aria-labelledby={`${id}-title`}>
      <p className="sr-only">{descriptions[family]}</p>
      <p>{family === "distribution" ? `n = ${analytics.completed}` : `n = ${n} · Range: ${windowSize === "50" ? "Recent 50" : "Full Run"}.`}{family === "net" && <Button size="sm" variant="ghost" aria-label={cyclingPaused ? "Resume cycling" : "Pause cycling"} aria-pressed={cyclingPaused} onClick={() => setCyclingPaused(paused => !paused)}>{cyclingPaused ? "Resume" : "Pause"} · 8s</Button>}</p>
      <Accordion className="my-4"><AccordionItem value="options"><AccordionTrigger aria-label={`${title} options`}>Chart options</AccordionTrigger><AccordionContent>
      <FieldGroup>
        {family !== "distribution" && <Field><FieldLabel htmlFor={`${id}-window`}>{title} window</FieldLabel><Select items={windowItems} value={windowSize} onValueChange={value => { if (value === "all" || value === "50") setWindowSize(value); }}><SelectTrigger id={`${id}-window`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{windowItems.map(item => <SelectItem key={item.value} value={item.value}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field>}
        {family === "net" ? <Field><FieldLabel htmlFor={`${id}-participant`}>Net score Participant</FieldLabel><Select items={participantItems} value={selected ?? ""} onValueChange={value => { if (typeof value === "string" && participants.some(p => p.participantId === value)) { setSelected(value); setManualRevision(revision => revision + 1); } }}><SelectTrigger id={`${id}-participant`}><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{participantItems.map(item => <SelectItem key={item.value} value={item.value} disabled={item.value === ""}>{item.label}</SelectItem>)}</SelectGroup></SelectContent></Select></Field> : <FieldSet><FieldLegend variant="label">Visible Participants — {title}</FieldLegend><div className="flex flex-wrap gap-x-6 gap-y-3">{participants.map(p => <FieldLabel key={p.participantId} className="min-h-11 items-center"><Checkbox aria-label={`${title} ${p.displayName}`} checked={!hidden.includes(p.participantId)} onCheckedChange={checked => setHidden(checked ? hidden.filter(id => id !== p.participantId) : [...hidden, p.participantId])} />{p.displayName}</FieldLabel>)}</div></FieldSet>}
      </FieldGroup>
      </AccordionContent></AccordionItem></Accordion>
      {family === "distribution" && distribution.length === 0 ? <Alert><AlertTitle>No Participants selected</AlertTitle></Alert> : <ChartContainer config={family === "distribution" ? rankConfig : config} className={`benchmark-chart-region max-sm:h-80! ${family === "distribution" ? "radar-host" : ""}`} aria-label={title} onClickCapture={event => event.currentTarget.querySelector<SVGSVGElement>(".recharts-surface")?.focus({ preventScroll: true })}>
        {family === "distribution" ? <RadarChart accessibilityLayer data={distribution} cx={radarGeometry.cx} cy={radarGeometry.cy} outerRadius={radarGeometry.radius} margin={{top: axisRem * 2, right: axisRem, bottom: axisRem / 2, left: axisRem}}><g aria-hidden="true" visibility="hidden">{participants.map(p => <text key={p.participantId} className="radar-label-metric" x={0} y={0}>{p.displayName}</text>)}</g><PolarGrid gridType="polygon" radialLines={false} /><PolarAngleAxis dataKey="name" tickLine={false} axisLine={false} tick={{width: axisRem * 2.4}} /><PolarRadiusAxis type="number" domain={[0,radarMaximum]} ticks={radarTicks} allowDataOverflow axisLine={false} angle={45} tick={props => <text className="recharts-polar-radius-axis-tick-value" x={props.x} y={Number(props.y) + (props.payload.value === 0 ? axisRem * .75 : props.payload.value === radarMaximum ? -axisRem * .6 : 0)} textAnchor={props.payload.value === radarMaximum ? "start" : "end"}>{props.payload.value}%</text>} />{tooltip}{legend}{Array.from({length: rankCount}, (_, i) => <Radar key={`r${i}`} dataKey={`r${i}`} name={`Rank ${i + 1}`} stroke={`var(--color-r${i})`} strokeWidth={2} fill={`var(--color-r${i})`} fillOpacity={0.08} dot={rankPoint(i)} activeDot={rankPoint(i,true)} isAnimationActive={false} />)}</RadarChart> : family === "net" ? <BarChart accessibilityLayer data={data} margin={plotMargin} barCategoryGap="25%" maxBarSize={32}>
          <CartesianGrid vertical={false} /><XAxis dataKey="sequence" tickLine={false} axisLine={false} tickMargin={10} interval="preserveStartEnd" minTickGap={axisRem / 2} height={axisRem * 2} /><YAxis width="auto" tickLine={false} axisLine={false} tickMargin={10} padding={{top: axisRem, bottom: axisRem * 1.5}} /><ReferenceLine y={0} ifOverflow="extendDomain" stroke="var(--border)" />{tooltip}{legend}
          {chosen && <Bar dataKey={`p${participants.indexOf(chosen)}`} name={chosen.displayName} radius={8} fill={`var(--color-p${participants.indexOf(chosen)})`} isAnimationActive={false} onClick={(_, index) => { const point = data[index]; if (point) onOpenMatch(point.matchId); }}>{showNetLabels && <LabelList className="net-value-label" position="top" offset={10} formatter={value => {const n=Number(value);return `${n>0?"+":""}${n/1000}k`;}} />}</Bar>}
        </BarChart> : <LineChart accessibilityLayer data={data} onClick={details} margin={plotMargin}>
          <CartesianGrid vertical={false} /><XAxis dataKey="sequence" tickLine={false} axisLine={false} tickMargin={10} interval="preserveStartEnd" minTickGap={axisRem / 2} height={axisRem * 2} /><YAxis width="auto" tickLine={false} axisLine={false} tickMargin={10} reversed={family === "movingRank"} domain={family === "movingRank" ? [1, rankCount] : ["auto", "auto"]} allowDecimals={family !== "movingRank"} />
          {family === "cumulative" && <ReferenceLine y={0} ifOverflow="extendDomain" stroke="var(--border)" />}{tooltip}{legend}
          {participants.map((p, i) => !hidden.includes(p.participantId) && <Line key={p.participantId} type="monotoneX" dataKey={`p${i}`} name={p.displayName} stroke={`var(--color-p${i})`} strokeWidth={2} dot={point(i)} activeDot={point(i, true)} isAnimationActive={false} />)}
        </LineChart>}
      </ChartContainer>}
    </section></CardContent>
  </Card>;
}

export function BenchmarkCharts({ analytics, onOpenMatch }: { analytics: Ready; onOpenMatch(matchId: string): void }) {
  const participants = [...analytics.participants].sort((a, b) => compareId(a.participantId, b.participantId));
  const [page, setPage] = useState(0);
  const [sequence, setSequence] = useState(1);
  const points = participants[0]?.points ?? [];
  const lastPage = Math.max(0, Math.ceil(points.length / 25) - 1);
  const currentPage = Math.min(page, lastPage);
  const rows = [...points].reverse().slice(currentPage * 25, currentPage * 25 + 25);
  const selected = points.find(point => point.sequence === sequence);
  return <section className="benchmark-dashboard mt-6 flex min-w-0 flex-col gap-6" aria-label="Benchmark analytics">
    {analytics.provisional && <Alert><AlertTitle>Provisional results</AlertTitle><AlertDescription>Sample n = {analytics.completed} completed Matches. Small samples do not establish playing strength.</AlertDescription></Alert>}
    <div className="benchmark-chart-hero"><ChartCard family="cumulative" analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} /></div>
    <div className="benchmark-chart-supporting">{(["net", "movingRank", "distribution"] as const).map(family => <ChartCard key={family} family={family} analytics={analytics} participants={participants} onOpenMatch={onOpenMatch} />)}</div>
    <Card><CardHeader><CardTitle><h2>Match history</h2></CardTitle></CardHeader><CardContent>
    <h2>Participant statistics</h2>
    <Table aria-label="Participant statistics" tabIndex={0}><TableHeader><TableRow><TableHead>Participant</TableHead><TableHead>Mean rank</TableHead><TableHead>First-place rate</TableHead><TableHead>Net score</TableHead>{participants.map((_, i) => <TableHead key={i}>Rank {i + 1} count / rate</TableHead>)}</TableRow></TableHeader><TableBody>
      {[...participants].sort((a, b) => (a.averageRank ?? Infinity) - (b.averageRank ?? Infinity) || compareId(a.participantId, b.participantId)).map(p => <TableRow key={p.participantId}><TableCell>{p.displayName}</TableCell><TableCell>{p.averageRank?.toFixed(2) ?? "—"}</TableCell><TableCell>{p.firstPlaceRate === null ? "—" : `${(p.firstPlaceRate * 100).toFixed(1)}%`}</TableCell><TableCell>{p.cumulativeNet.toLocaleString()}</TableCell>{p.rankCounts.map((count, i) => <TableCell key={i}>{count} / {analytics.completed ? (count / analytics.completed * 100).toFixed(1) : "0.0"}%</TableCell>)}</TableRow>)}
    </TableBody></Table>
    <section id="benchmark-numeric" tabIndex={0} role="region" aria-label="Scrollable Match results">
      <FieldGroup><Field><FieldLabel htmlFor="benchmark-sequence">Selected Match sequence</FieldLabel><Input id="benchmark-sequence" className="max-w-48" type="number" min={1} max={Math.max(1, points.length)} value={sequence} onChange={e => setSequence(Number(e.target.value))} /></Field></FieldGroup>
      <Button variant="outline" className="h-auto min-h-10 max-w-full whitespace-normal" disabled={!selected} onClick={() => { if (selected) onOpenMatch(selected.matchId); }}>View Match details</Button>
      <p>Recent-first, 25 Matches per page. Net / full-run cumulative / trailing mean rank (sample n).</p>
      <Table aria-label="Per-Match numeric results"><TableHeader><TableRow><TableHead>Match</TableHead>{participants.map(p => <TableHead key={p.participantId}>{p.displayName}</TableHead>)}</TableRow></TableHeader><TableBody>{rows.map(point => <TableRow key={point.matchId}><TableCell><Button variant="outline" onClick={() => onOpenMatch(point.matchId)}>View Match {point.sequence} details</Button></TableCell>{participants.map(p => {
        const value = p.points[point.sequence - 1];
        return <TableCell key={p.participantId}>{value.net.toLocaleString()} / {value.cumulative.toLocaleString()} / {value.movingRank.toFixed(2)} (n={value.movingN})</TableCell>;
      })}</TableRow>)}</TableBody></Table>
      <div className="action-row"><Button variant="outline" disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>Newer Matches</Button><span>Page {currentPage + 1} of {lastPage + 1}</span><Button variant="outline" disabled={currentPage >= lastPage} onClick={() => setPage(currentPage + 1)}>Older Matches</Button></div>
    </section></CardContent></Card>
  </section>;
}
