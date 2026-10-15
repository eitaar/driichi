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
 expect(detail).toHaveTextContent('P1');
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
 expect(detail).toHaveTextContent('connected');
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
 expect(await screen.findByRole('dialog', { name: 'Mika' })).toHaveTextContent('P1');
});

it('keeps duplicate display names mapped to independent Participant IDs', async () => {
 const user = userEvent.setup();
 render(<><LobbyParticipantCard participant={participant} own /><LobbyParticipantCard participant={{ ...participant, participant_id: 'P2' }} own={false} /></>);
 const triggers = screen.getAllByRole('button', { name: 'Mika' });
 await user.click(triggers[0]);
 expect(await screen.findByRole('dialog')).toHaveTextContent('P1');
 expect(screen.getByRole('dialog')).not.toHaveTextContent('P2');
 await user.keyboard('{Escape}');
 await user.click(triggers[1]);
 expect(await screen.findByRole('dialog')).toHaveTextContent('P2');
 expect(screen.getByRole('dialog')).not.toHaveTextContent('P1');
});
