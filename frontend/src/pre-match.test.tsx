import {fireEvent,render,screen,waitFor} from '@testing-library/react';
import {beforeEach,it,expect,vi} from 'vitest';
import {App} from './app';
const characters=[{id:'player-red',name:'Red Player'},{id:'tsumogiri-bot',name:'Tsumogiri'}];
beforeEach(()=>{
 window.history.replaceState({},'','/');
 vi.stubGlobal('fetch',vi.fn(async(url)=>new Response(JSON.stringify(String(url).includes('characters')?characters:{room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4}),{headers:{'content-type':'application/json'}})));
});
it('entry presents registry artwork while retaining six-digit room navigation',async()=>{
 render(<App/>);
 await waitFor(()=>expect(screen.getByRole('heading',{name:'Join a room'})).toBeVisible());
 await waitFor(()=>expect(document.querySelectorAll('.entry-cast img')).toHaveLength(2));
 expect(document.querySelector('.entry-cast img')).toHaveAttribute('src','/assets/characters/player-red/portrait.webp');
 fireEvent.change(screen.getByLabelText('Room code'),{target:{value:'12'}});
 fireEvent.click(screen.getByRole('button',{name:'Open room'}));
 expect(screen.getByRole('alert')).toHaveTextContent('six-digit');
 expect(window.location.pathname).toBe('/');
});
it('character choice shows a separate portrait and keeps the entered display name',async()=>{
 window.history.replaceState({},'','/room/123456');render(<App/>);
 await screen.findByRole('heading',{name:'Choose your character'});
 fireEvent.change(screen.getByLabelText('Display name'),{target:{value:'Mika'}});
 fireEvent.click(await screen.findByRole('radio',{name:'Red Player'}));
 expect(document.querySelector('.character-stage img')).toHaveAttribute('src','/assets/characters/player-red/portrait.webp');
 fireEvent.click(screen.getByRole('radio',{name:'Tsumogiri'}));
 expect(screen.getByLabelText('Display name')).toHaveValue('Mika');
 expect(document.querySelector('.character-stage img')).toHaveAttribute('src','/assets/characters/tsumogiri-bot/portrait.webp');
 expect(screen.queryByText('Night Market')).not.toBeInTheDocument();
 expect(screen.queryByText('ROOM / 123456')).not.toBeInTheDocument();
});
it('stops an old voice preview when switching characters',async()=>{
 const pause=vi.fn();const play=vi.fn(()=>Promise.resolve());
 vi.stubGlobal('Audio',vi.fn(function(){return {play,pause,onended:null};}));
 window.history.replaceState({},'','/room/123456');render(<App/>);
 fireEvent.click(await screen.findByRole('radio',{name:'Red Player'}));
 fireEvent.click(screen.getByRole('button',{name:'Preview riichi voice'}));
 await screen.findByRole('button',{name:'Playing riichi voice'});
 fireEvent.click(screen.getByRole('radio',{name:'Tsumogiri'}));
 expect(pause).toHaveBeenCalled();
 expect(screen.getByRole('button',{name:'Preview riichi voice'})).toBeVisible();
});
