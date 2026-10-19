import { useState } from 'react';
import type { RoomParticipant } from '../api';
import { Button } from './ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';

export function LobbyParticipantCard({ participant, own, previousSeat }: { participant: RoomParticipant; own: boolean; previousSeat?: number }) {
 const [failed, setFailed] = useState(false);
 return <Dialog>
  <DialogTrigger render={<Button variant="ghost" className="lobby-participant-card relative isolate flex h-auto w-full min-w-0 flex-col gap-0 p-0 whitespace-normal" />} aria-label={participant.display_name}>
   <span className="lobby-center-art">
    <span className="lobby-center-unskew">{failed ? <span className="flex h-full items-center justify-center text-muted-foreground">Portrait unavailable</span> : <img className="lobby-portrait" src={`/assets/characters/${encodeURIComponent(participant.character_id)}/portrait.webp`} alt="" onError={() => setFailed(true)} />}</span>
    <span className="lobby-center-name">{participant.display_name}</span>
   </span>
  </DialogTrigger>
  <DialogContent>
   <DialogHeader><DialogTitle>{participant.display_name}</DialogTitle><DialogDescription>{own ? 'Your Participant' : 'Participant details'}</DialogDescription></DialogHeader>
   <dl className="lobby-participant-states">
    <div><dt>Kind</dt><dd>{participant.kind.replaceAll('_', ' ')}</dd></div>
    {participant.presence !== 'connected' && <div><dt>Presence</dt><dd>{participant.presence}</dd></div>}
    <div><dt>Selection</dt><dd>{participant.selected ? 'selected' : 'unselected'}</dd></div>
    <div><dt>Ready</dt><dd>{participant.ready ? 'ready' : 'not ready'}</dd></div>
    <div><dt>Controller</dt><dd>{participant.controller.replaceAll('_', ' ')}</dd></div>
    {previousSeat !== undefined && <div><dt>Previous Match Seat</dt><dd>Seat {previousSeat + 1} · not assigned for the next Match</dd></div>}
   </dl>
  </DialogContent>
 </Dialog>;
}
