import { useState } from 'react';
import type { RoomParticipant } from '../api';
import { Button } from './ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from './ui/dialog';

export function LobbyParticipantCard({ participant, own, previousSeat }: { participant: RoomParticipant; own: boolean; previousSeat?: number }) {
 const [failed, setFailed] = useState(false);
 return <Dialog>
  <DialogTrigger render={<Button variant="ghost" className="lobby-participant-card relative isolate flex h-auto w-full min-w-0 flex-col gap-0 p-0 whitespace-normal" />} aria-label={participant.display_name}>
   <span aria-hidden="true" className="lobby-portrait-frame pointer-events-none absolute inset-0 -z-10 -skew-x-4 rounded-lg border bg-muted/30 md:-skew-x-8" />
   <span className="relative block aspect-square max-h-[36vh] w-full overflow-hidden pt-3">
    {failed ? <span className="flex h-full items-center justify-center text-muted-foreground">Portrait unavailable</span> : <img className="lobby-portrait ml-[-5%] block h-auto w-[110%] max-w-none" src={`/assets/characters/${encodeURIComponent(participant.character_id)}/portrait.webp`} alt="" onError={() => setFailed(true)} />}
   </span>
   <span className="block w-full px-3 py-3 text-base wrap-anywhere">{participant.display_name}</span>
  </DialogTrigger>
  <DialogContent>
   <DialogHeader><DialogTitle>{participant.display_name}</DialogTitle><DialogDescription>{own ? 'Your Participant' : 'Participant details'}</DialogDescription></DialogHeader>
   <dl className="lobby-participant-states">
    <div><dt>Identity</dt><dd>{participant.participant_id}</dd></div>
    <div><dt>Kind</dt><dd>{participant.kind.replaceAll('_', ' ')}</dd></div>
    <div><dt>Presence</dt><dd>{participant.presence}</dd></div>
    <div><dt>Selection</dt><dd>{participant.selected ? 'selected' : 'unselected'}</dd></div>
    <div><dt>Ready</dt><dd>{participant.ready ? 'ready' : 'not ready'}</dd></div>
    <div><dt>Controller</dt><dd>{participant.controller.replaceAll('_', ' ')}</dd></div>
    {previousSeat !== undefined && <div><dt>Previous Match Seat</dt><dd>Seat {previousSeat + 1} · not assigned for the next Match</dd></div>}
   </dl>
  </DialogContent>
 </Dialog>;
}
