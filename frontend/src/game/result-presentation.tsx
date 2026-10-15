import { useState } from "react";
import { Button } from "../components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "../components/ui/toggle-group";
import { Drawer, DrawerTrigger, DrawerContent, DrawerTitle, DrawerDescription, DrawerClose } from "../components/ui/drawer";
import { decodeKyokuEndStep, decodeRoomFinalStandings, resultRoster, resultStepKey, type KyokuSettlement, type WinResult } from "./results";
import type { RoomSnapshot } from "./types";
import { tileAssetUrl, tileLabel } from "./tiles";
import "./result.css";

export interface AssetState { [characterId: string]: boolean }

function ResultPortrait({ characterId, name, available, className = "" }: {
  characterId: string | null | undefined; name: string; available: boolean; className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const portraitClass = `results-winner-portrait ${className}`;
  if (!characterId || !available || failed) return <span className={`${portraitClass} asset-fallback`} role="img" aria-label={`${name} portrait unavailable`}>{Array.from(name.trim())[0]?.toUpperCase() ?? "?"}</span>;
  return <img className={portraitClass} src={`/assets/characters/${encodeURIComponent(characterId)}/portrait.webp`} alt={`${name} portrait`} onError={() => setFailed(true)} />;
}

function yakuLabel(name: string) {
  return name.replaceAll("_", " ").replace(/\s+/g, " ").trim().replace(/(^|\s)\S/g, letter => letter.toUpperCase());
}

function RoundWinTiles({ result }: { result: WinResult }) {
  return <div className="round-win-hand" role="group" aria-label="Winning hand">
    <div className="round-win-tile-row" role="group" aria-label={`${result.concealed.length} visible winning hand tiles`}>
      {result.concealed.map((tile, index) => <span className={`round-win-tile${result.winning_tile_index === index ? " is-winning" : ""}`} data-testid={result.winning_tile_index === index ? "winning-tile" : undefined} key={`${tile}-${index}`}><img src={tileAssetUrl(tile)} alt={tileLabel(tile)} /></span>)}
    </div>
    {result.melds.length > 0 && <div className="round-win-melds" role="group" aria-label="Visible melds">{result.melds.map((meld, index) => <span className="round-win-meld" key={index} role="group" aria-label={`${meld.opened ? "Open" : "Closed"} ${meld.kind}`}>
      {meld.tiles.map((tile, tileIndex) => <img key={`${tile}-${tileIndex}`} src={!meld.opened && meld.kind === "Ankan" && (tileIndex === 0 || tileIndex === meld.tiles.length - 1) ? new URL("../assets/tiles/Regular/Back.svg", import.meta.url).href : tileAssetUrl(tile)} alt={tileLabel(tile)} />)}
    </span>)}</div>}
  </div>;
}

export function RoundWinSurface({ result, room, assets, reducedMotion = false }: { result: WinResult; room: RoomSnapshot | null; assets: AssetState; reducedMotion?: boolean }) {
  const winner = resultRoster(room)?.find(player => player.participant_id === result.winner && player.seat === result.seat);
  if (!winner) return null;
  const ordinary = "Ordinary" in result.evaluation ? result.evaluation.Ordinary : null;
  return <section className="round-win-surface" data-testid="round-win-surface" data-motion={reducedMotion ? "static" : "cinematic"} aria-labelledby="round-win-heading">
    <div className="round-win-identity"><ResultPortrait characterId={winner.character_id} name={winner.display_name} available={winner.character_id ? assets[winner.character_id] !== false : false} className="round-win-portrait" /><h2 id="round-win-heading">{winner.display_name}</h2></div>
    <div className="round-win-details" aria-label={`${result.tsumo ? "Tsumo" : "Ron"} win details`}>
      <RoundWinTiles result={result} />
      {result.yaku.length > 0 && <dl className="round-win-yaku" aria-label="Authoritative yaku">{result.yaku.map((entry, index) => <div key={`${entry.name}-${index}`}><dt>{yakuLabel(entry.name)}</dt><dd>{entry.han !== undefined ? `${entry.han} han` : entry.yakuman_multiplier !== undefined ? `${entry.yakuman_multiplier}× yakuman` : ""}</dd></div>)}</dl>}
      <div className="round-win-total"><p>{ordinary ? <>{ordinary.han} han · {ordinary.fu} fu{ordinary.limit && ` · ${ordinary.limit}`}</> : "Yakuman" in result.evaluation && `${result.evaluation.Yakuman.multiplier}× yakuman`}</p><strong>{result.win_points.toLocaleString()} <small>points</small></strong></div>
    </div>
  </section>;
}

export function ScoreTransfer({ settlement, room, progress = 1 }: { settlement: KyokuSettlement; room: RoomSnapshot | null; progress?: number }) {
  const players = resultRoster(room);
  if (!players || settlement.changes.length !== players.length || new Set(settlement.changes.map(change => change.participant_id)).size !== players.length || new Set(settlement.changes.map(change => change.seat)).size !== players.length || settlement.changes.some(change => !players.some(player => player.seat === change.seat && player.participant_id === change.participant_id))) return null;
  return <section className="score-transfer" aria-label="Score transfer">
    <h2>{settlement.draw_reason ?? "Score transfer"}</h2>
    <div className="score-transfer-labels" aria-hidden="true"><span>Player</span><span>Before</span><span>Change</span><span>After</span></div>
    <ol>{settlement.changes.map(entry => {
      const player = players.find(player => player.participant_id === entry.participant_id)!;
      const change = Math.round(entry.delta * Math.max(0, Math.min(1, progress)));
      return <li key={entry.participant_id}><span>{player.display_name}</span><span aria-label={`Before ${entry.before.toLocaleString()}`}>{entry.before.toLocaleString()}</span><span aria-label={`Change ${entry.delta >= 0 ? "+" : "−"}${Math.abs(entry.delta).toLocaleString()}`}>{entry.delta >= 0 ? "+" : "−"}{Math.abs(change).toLocaleString()}</span><strong aria-label={`After ${entry.after.toLocaleString()}`}>{(entry.before + change).toLocaleString()}</strong></li>;
    })}</ol>
  </section>;
}

export function DrawResult({ settlement, room }: { settlement: KyokuSettlement; room: RoomSnapshot | null }) {
  const [view, setView] = useState("result");
  return <section className="draw-result" aria-label="Draw result">
    <ToggleGroup aria-label="Draw view" value={[view]} onValueChange={values => { if (values.length) setView(values[0]); }}><ToggleGroupItem value="result">Result</ToggleGroupItem><ToggleGroupItem value="scores">Scores</ToggleGroupItem></ToggleGroup>
    {view === "result" ? <div className="draw-reason"><h2>{settlement.draw_reason ?? "Draw"}</h2><img src={new URL("../assets/tiles/Regular/Back.svg", import.meta.url).href} alt="" /></div> : <ScoreTransfer settlement={settlement} room={room} />}
  </section>;
}

export function ResultsPanel({ room, assets }: { room: RoomSnapshot | null; assets: AssetState }) {
  const result = decodeRoomFinalStandings(room);
  if (!result) return null;
  const ordered = result.players.slice().sort((left, right) => left.rank - right.rank);
  const first = ordered.find(player => player.rank === 1)!;
  const firstAsset = resultRoster(room)!.find(entry => entry.participant_id === first.participant_id)?.character_id;
  return <section className="results-panel" data-testid="results-panel" aria-labelledby="results-heading">
    <div className="results-winner"><ResultPortrait characterId={firstAsset} name={first.display_name} available={firstAsset ? assets[firstAsset] !== false : false} className="results-winner-hero-portrait" /></div>
    <div className="results-standings"><h2 id="results-heading">Final standings</h2><ol className="results-list" aria-label="Final standings">{ordered.map(player => <li key={player.participant_id} className={player.rank === 1 ? "is-first" : undefined}><span className="result-rank" aria-label={`Rank ${player.rank}`}>{player.rank}</span><span className="result-name">{player.display_name}</span><span className="result-score">{player.final_score.toLocaleString()}</span></li>)}</ol></div>
  </section>;
}

/** Read-only authorized metadata; deliberately no live controller or renderer dependency. */
export function MobileResultSummary({ room, onReturn }: { room: RoomSnapshot; onReturn?: () => void }) {
  const step = decodeKyokuEndStep(room.kyoku_end, room.game_mode);
  const final = room.phase === "post_match" && decodeRoomFinalStandings(room);
  const win = step && "Win" in step.content ? step.content.Win : null;
  const settlement = step && !("Win" in step.content) ? ("Draw" in step.content ? step.content.Draw : step.content.Settlement) : null;
  const winner = win && resultRoster(room)?.find(player => player.participant_id === win.winner && player.seat === win.seat);
  if (!final && !winner && !settlement) return null;
  const title = final ? "Final standings" : winner ? `${winner.display_name} · ${win!.tsumo ? "Tsumo" : "Ron"}` : settlement?.draw_reason ?? (step && "Draw" in step.content ? "Draw" : "Score transfer");
  return <section className="flat-section mobile-result" aria-label="Match result">
    <h2>{title}</h2><p>Read-only result</p>
    {final && <ol aria-label="Final standings">{final.players.slice().sort((a, b) => a.rank - b.rank).map(player => <li className="flex gap-4" key={player.participant_id}><span aria-label={`Rank ${player.rank}`}>{player.rank}</span><span>{player.display_name}</span><span>{player.final_score.toLocaleString()}</span></li>)}</ol>}
    <Drawer key={final ? "final" : step ? resultStepKey(step.id) : "none"}>
      <DrawerTrigger render={<Button variant="outline" />}>Expand</DrawerTrigger>
      <DrawerContent className="overflow-y-auto"><DrawerTitle>{title}</DrawerTitle><DrawerDescription>Authoritative result details. Live confirmation is available on desktop only.</DrawerDescription>
        <div className="result-details">{final ? <ResultsPanel room={room} assets={{}} /> : win ? <RoundWinSurface result={win} room={room} assets={{}} reducedMotion /> : settlement && <ScoreTransfer settlement={settlement} room={room} />}</div>
        <DrawerClose render={<Button variant="outline" />}>Close</DrawerClose>
      </DrawerContent>
    </Drawer>
    {onReturn && <Button variant="outline" onClick={onReturn}>Return to Room</Button>}
  </section>;
}
