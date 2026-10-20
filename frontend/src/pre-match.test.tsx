import {act,fireEvent,render,screen,waitFor} from '@testing-library/react';
import {beforeEach,it,expect,vi} from 'vitest';
import {App} from './app';
const characters=[{id:'player-red',name:'Red Player'},{id:'tsumogiri-bot',name:'Tsumogiri'}];
const room={room_name:'Night Market',game_mode:'4p-red-east',phase:'Lobby',join_allowed:true,participant_count:1,participant_limit:4};
beforeEach(()=>{
 window.history.replaceState({},'','/');
 sessionStorage.clear();
 vi.stubGlobal('fetch',vi.fn(async(url)=>new Response(JSON.stringify(String(url).includes('characters')?characters:room),{headers:{'content-type':'application/json'}})));
});
async function fillJoinName(){
 await screen.findByLabelText('Display name');
 fireEvent.change(screen.getByLabelText('Display name'),{target:{value:'Mika'}});
}
it('entry puts functional Join before decorative artwork in reading order and associates invalid code errors',async()=>{
 render(<App/>);
 await waitFor(()=>expect(document.querySelectorAll('.entry-cast img')).toHaveLength(2));
 const input=screen.getByLabelText('Room code');
 expect(document.querySelector('main')!.firstElementChild).toContainElement(input);
 expect(input).toHaveFocus();
 fireEvent.change(input,{target:{value:'12'}});
 fireEvent.click(screen.getByRole('button',{name:'Open room'}));
 expect(input).toHaveAttribute('aria-invalid','true');
 expect(input).toHaveAccessibleDescription(/six-digit/);
 expect(screen.getByRole('alert')).toHaveTextContent('six-digit');
 expect(window.location.pathname).toBe('/');
});
it('shows name and character together and Back returns to entry',async()=>{
 window.history.replaceState({},'','/room/123456');render(<App/>);
 await screen.findByRole('heading',{name:'Join Night Market'});
 expect(screen.getByRole('radio',{name:'Red Player'})).toBeVisible();
 fireEvent.blur(screen.getByLabelText('Display name'));
 expect(screen.getByLabelText('Display name')).toHaveAttribute('aria-invalid','true');
 expect(screen.getByLabelText('Display name')).toHaveAccessibleDescription(/enter a display name/i);
 await fillJoinName();
 fireEvent.click(screen.getByRole('radio',{name:'Red Player'}));
 expect(document.querySelector('.character-stage img')).toHaveAttribute('src','/assets/characters/player-red/portrait.webp');
 expect(screen.getByLabelText('Display name')).toHaveValue('Mika');
 expect(screen.getByRole('radio',{name:'Red Player'})).toBeChecked();
 expect(window.location.pathname).toBe('/room/123456');
 fireEvent.click(screen.getByRole('button',{name:'Back'}));
 expect(screen.getByRole('heading',{name:'Join a room'})).toBeVisible();
 expect(window.location.pathname).toBe('/');
});
it('retains rejected join input and associates the inline error with Character',async()=>{
 vi.stubGlobal('fetch',vi.fn(async(url)=>String(url).endsWith('/join')?new Response(JSON.stringify({title:'Join rejected',detail:'This room is now full.',code:'room_full'}),{status:409,headers:{'content-type':'application/problem+json'}}):new Response(JSON.stringify(String(url).includes('characters')?characters:room),{headers:{'content-type':'application/json'}})));
 window.history.replaceState({},'','/room/123456');render(<App/>);await fillJoinName();
 fireEvent.click(screen.getByRole('radio',{name:'Red Player'}));
 fireEvent.click(screen.getByRole('button',{name:'Join room'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('This room is now full.');
 expect(screen.getByRole('radiogroup')).toHaveAccessibleDescription('This room is now full.');
 expect(screen.getByRole('radio',{name:'Red Player'})).toBeChecked();
 expect(screen.getByRole('button',{name:'Join room'})).toBeEnabled();
 expect(screen.getByLabelText('Display name')).toHaveValue('Mika');
});
it('does not present character or join controls when joining is forbidden',async()=>{
 vi.stubGlobal('fetch',vi.fn(async()=>new Response(JSON.stringify({...room,join_allowed:false}),{headers:{'content-type':'application/json'}})));
 window.history.replaceState({},'','/room/123456');render(<App/>);
 expect(await screen.findByRole('status')).toHaveTextContent('full');
 expect(screen.queryByRole('textbox')).not.toBeInTheDocument();
 expect(screen.queryByRole('button',{name:'Join room'})).not.toBeInTheDocument();
});
it('stops an old voice preview when switching characters',async()=>{
 const pause=vi.fn();const play=vi.fn(()=>Promise.resolve());
 vi.stubGlobal('Audio',vi.fn(function(){return {play,pause,onended:null};}));
 window.history.replaceState({},'','/room/123456');render(<App/>);await fillJoinName();
 fireEvent.click(screen.getByRole('radio',{name:'Red Player'}));
 fireEvent.click(screen.getByRole('button',{name:'Preview riichi voice'}));
 await screen.findByRole('button',{name:'Playing riichi voice'});
 fireEvent.click(screen.getByRole('radio',{name:'Tsumogiri'}));
 expect(pause).toHaveBeenCalled();
 expect(screen.getByRole('button',{name:'Preview riichi voice'})).toBeVisible();
});
it('reports unavailable portrait and rejected voice without claiming playback',async()=>{
 vi.stubGlobal('Audio',vi.fn(function(){return {play:()=>Promise.reject(new Error('missing voice')),pause:vi.fn(),onended:null};}));
 window.history.replaceState({},'','/room/123456');render(<App/>);await fillJoinName();
 fireEvent.click(screen.getByRole('radio',{name:'Red Player'}));
 fireEvent.error(screen.getByAltText('Red Player portrait'));
 expect(screen.getByLabelText('Red Player portrait unavailable')).toBeVisible();
 fireEvent.click(screen.getByRole('button',{name:'Preview riichi voice'}));
 await screen.findByRole('button',{name:'Voice preview is unavailable'});
 expect(screen.queryByRole('button',{name:'Playing riichi voice'})).not.toBeInTheDocument();
 expect(screen.getByRole('button',{name:'Join room'})).toBeEnabled();
});
it('does not save a late accepted join after browser navigation leaves Join',async()=>{
 let resolveJoin!: (value:Response)=>void;
 vi.stubGlobal('fetch',vi.fn(async(url)=>String(url).endsWith('/join')?new Promise<Response>(done=>{resolveJoin=done;}):new Response(JSON.stringify(String(url).includes('characters')?characters:room),{headers:{'content-type':'application/json'}})));
 window.history.replaceState({},'','/room/123456');render(<App/>);await fillJoinName();
 fireEvent.click(screen.getByRole('radio',{name:'Red Player'}));
 fireEvent.click(screen.getByRole('button',{name:'Join room'}));
 expect(screen.getByRole('button',{name:'Joining room'})).toBeDisabled();
 expect(screen.getByRole('button',{name:'Back'})).toBeDisabled();
 fireEvent.click(screen.getByRole('radio',{name:'Tsumogiri'}));
 expect(screen.getByRole('radio',{name:'Red Player'})).toBeChecked();
 act(()=>{window.history.replaceState({},'','/');window.dispatchEvent(new PopStateEvent('popstate'));});
 await act(async()=>resolveJoin(new Response(JSON.stringify({participant_id:'P1',websocket_url:'/ws/v1/rooms/123456/human'}),{status:201,headers:{'content-type':'application/json'}})));
 expect(screen.getByRole('heading',{name:'Join a room'})).toBeVisible();
 expect(sessionStorage.getItem('driichi:participant:123456')).toBeNull();
});
it('ignores late lookup responses after navigating away',async()=>{
 let resolve!: (value:Response)=>void;
 vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(done=>{resolve=done;})));
 window.history.replaceState({},'','/room/123456');render(<App/>);
 act(()=>{window.history.replaceState({},'','/');window.dispatchEvent(new PopStateEvent('popstate'));});
 await act(async()=>resolve(new Response(JSON.stringify(room),{headers:{'content-type':'application/json'}})));
 expect(screen.getByRole('heading',{name:'Join a room'})).toBeVisible();
 expect(screen.queryByLabelText('Display name')).not.toBeInTheDocument();
});
