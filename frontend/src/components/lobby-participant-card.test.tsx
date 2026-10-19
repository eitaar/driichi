import { fireEvent, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { expect, it } from 'vitest';
import { LobbyParticipantCard } from './lobby-participant-card';
import type { RoomParticipant } from '../api';

const participant: RoomParticipant = { participant_id: 'P1', display_name: 'Mika', kind: 'human', presence: 'disconnected', selected: true, ready: false, character_id: 'player-red', role: 'none', controller: 'interactive' };

it('defaults to portrait and name only; keyboard opens exact participant states and restores focus', async () => {
 const user = userEvent.setup();
 render(<LobbyParticipantCard participant={participant} own previousSeat={2} />);
 const trigger = screen.getByRole('button', { name: 'Mika' });
 expect(trigger).toHaveTextContent(/^Mika$/);
 expect(screen.queryByText(/ready|you|seat|human|disconnected/i)).not.toBeInTheDocument();
 expect(within(trigger).getByAltText('')).toHaveAttribute('src', '/assets/characters/player-red/portrait.webp');
 trigger.focus();
 await user.keyboard('{Enter}');
 const detail = await screen.findByRole('dialog', { name: 'Mika' });
 expect(detail).not.toHaveTextContent('P1');
 expect(detail).toHaveTextContent('disconnected');
 expect(detail).toHaveTextContent('selected');
 expect(detail).toHaveTextContent('not ready');
 expect(detail).toHaveTextContent('interactive');
 expect(detail).toHaveTextContent('Your Participant');
 expect(detail).toHaveTextContent('Previous Match Seat');
 expect(detail).toHaveTextContent('Seat 3');
 await user.keyboard('{Escape}');
 expect(trigger).toHaveFocus();
});

it('touch/click details have no next Seat and show exact unselected, ready and controller states', async () => {
 render(<LobbyParticipantCard participant={{ ...participant, selected: false, ready: true, presence: 'connected', controller: 'temporary_auto' }} own={false} />);
 fireEvent.click(screen.getByRole('button', { name: 'Mika' }));
 const detail = await screen.findByRole('dialog');
 expect(detail).toHaveTextContent('unselected');
 expect(detail).not.toHaveTextContent('connected');
 expect(within(detail).queryByText('Presence')).not.toBeInTheDocument();
 expect(detail).toHaveTextContent('ready');
 expect(detail).toHaveTextContent('temporary auto');
 expect(detail).not.toHaveTextContent(/Seat|Your Participant/);
});

it('keeps participant name and details available when the portrait is missing', async () => {
 render(<LobbyParticipantCard participant={participant} own={false} />);
 fireEvent.error(screen.getByAltText(''));
 expect(screen.getByRole('button', { name: 'Mika' })).toBeVisible();
 expect(screen.getByRole('button', { name: 'Mika' })).toHaveTextContent('Portrait unavailable');
 fireEvent.click(screen.getByRole('button', { name: 'Mika' }));
 expect(await screen.findByRole('dialog', { name: 'Mika' })).not.toHaveTextContent('P1');
});

it('keeps duplicate display names mapped to independent Participant details without displaying IDs', async () => {
 const user = userEvent.setup();
 render(<><LobbyParticipantCard participant={participant} own /><LobbyParticipantCard participant={{ ...participant, participant_id: 'P2', ready: true }}  own={false} /></>);
 const triggers = screen.getAllByRole('button', { name: 'Mika' });
 await user.click(triggers[0]);
 expect(await screen.findByRole('dialog')).toHaveTextContent('not ready');
 expect(screen.getByRole('dialog')).not.toHaveTextContent(/P1|P2|Identity/);
 await user.keyboard('{Escape}');
 await user.click(triggers[1]);
 expect(await screen.findByRole('dialog')).not.toHaveTextContent('not ready');
 expect(screen.getByRole('dialog')).not.toHaveTextContent(/P1|P2|Identity/);
});

it('owns the name and unskewed original art inside the whole clipped diagonal frame', () => {
 render(<LobbyParticipantCard participant={participant} own={false} />);
 const trigger = screen.getByRole('button', { name: 'Mika' });
 const art = trigger.querySelector('.lobby-center-art')!;
 expect(art).not.toBeNull();
 expect(art.querySelector('.lobby-center-unskew > .lobby-portrait')).not.toBeNull();
 expect(art.querySelector('.lobby-center-name')).toHaveTextContent('Mika');
});

it('keeps a 64-character name accessible through Space, Escape and focus return', async () => {
 const user = userEvent.setup();
 const display_name = 'N'.repeat(64);
 render(<LobbyParticipantCard participant={{ ...participant, display_name }} own={false} />);
 const trigger = screen.getByRole('button', { name: display_name });
 trigger.focus();
 await user.keyboard(' ');
 expect(await screen.findByRole('dialog', { name: display_name })).toBeVisible();
 await user.keyboard('{Escape}');
 expect(trigger).toHaveFocus();
});
