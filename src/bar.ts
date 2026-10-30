// The bar: current task and countdown, floating just above the taskbar.
import { getCurrentWindow, PhysicalPosition, LogicalSize, primaryMonitor } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { listen, emit } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { catSvg } from './cat';
import { TimerState, idle, start, remaining, toggle, format, nudge, elapsedMs, runEnded } from './timer';
import { addSpent, alreadyLogged } from './prefs';

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id)!;
const store = {
  get<T>(k: string, d: T): T { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch { return d; } },
  set(k: string, v: unknown) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* storage off: fine */ } },
};

let state: TimerState = store.get('timer', idle());
let rang = false;
// "native": the native bar (mazecat-bar.exe) draws on the taskbar and this window stays
// hidden, keeping the logic. "web": this window is the bar, as before.
let native = false;

function pushNative() {
  if (native) invoke('nbar_send', { msg: { t: 'state', ...state, def: defaultMin } }).catch(() => {});
}

function render() {
  if (native) return;
  const left = remaining(state);
  const task = $('task'), clock = $('clock');
  const up = !!state.task && state.totalMs > 0 && left === 0;
  task.textContent = state.task ? (up ? `Time's up · ${state.task}` : state.task) : 'Pick a task';
  task.title = state.task ? `${state.project} · ${state.task}\nDouble-click to pick another task` : 'Double-click to pick a task';
  task.classList.toggle('idle', !state.task);
  clock.textContent = state.task ? format(left) : '--:--';
  clock.className = 'clock' + (up ? ' up' : state.endsAt !== null ? ' running' : state.task ? ' paused' : '');
  $('cat').innerHTML = catSvg(up ? 'awake' : 'calm');
  if (up && !rang) { rang = true; chime(); emit('timer-up', { task: state.task, project: state.project }); if (anchor) emit('ask-open', { task: state.task, project: state.project, minutes: state.totalMs / 60_000, anchor }); }
  if (!up) rang = false;
  fit();
}

function set(s: TimerState) {
  // A run that ends adds its time to its task, for the time-spent log in Jira.
  if (runEnded(state, s) && !alreadyLogged(state)) addSpent(state.project, state.task, elapsedMs(state));
  state = s; store.set('timer', s); render(); pushNative(); emit('timer-state', s); }

// A soft two-note chime made on the fly (no sound files to license).
function chime() {
  try {
    const ac = new AudioContext();
    [660, 880].forEach((f, i) => {
      const o = ac.createOscillator(), g = ac.createGain(), t = ac.currentTime + i * 0.28;
      o.frequency.value = f; o.type = 'sine';
      g.gain.setValueAtTime(0.0001, t); g.gain.exponentialRampToValueAtTime(0.18, t + 0.03); g.gain.exponentialRampToValueAtTime(0.0001, t + 0.9);
      o.connect(g).connect(ac.destination); o.start(t); o.stop(t + 1);
    });
  } catch { /* no audio: the bar still changes color */ }
}

async function toggleNotes() {
  const notes = await WebviewWindow.getByLabel('notes');
  if (!notes) return;
  if (await notes.isVisible()) await notes.hide();
  else { await notes.show(); await notes.unminimize(); await notes.setFocus(); }
}

// The window hugs its content and keeps its right edge where you left it.
const HEIGHT = 40;
let anchor: { right: number; y: number } | null = store.get('barAnchor', null);
let lastW = 0, fitting = false;

async function fit() {
  const el = ($('presets').hidden ? $('bar') : $('presets')) as HTMLElement;
  const w = Math.ceil(el.getBoundingClientRect().width) + 2;
  if (!anchor || fitting || Math.abs(w - lastW) < 2) return;
  fitting = true; lastW = w;
  try {
    const scale = await win.scaleFactor();
    await win.setSize(new LogicalSize(w, HEIGHT));
    await win.setPosition(new PhysicalPosition(anchor.right - Math.round(w * scale), anchor.y));
  } finally { fitting = false; }
}

async function ensureAnchor() {
  if (!anchor) {
    const m = await primaryMonitor();
    if (!m) return;
    // Bottom right, over the taskbar, leaving room for the clock and tray icons.
    anchor = {
      right: m.position.x + m.size.width - Math.round(260 * m.scaleFactor),
      y: m.position.y + m.size.height - Math.round((HEIGHT + 4) * m.scaleFactor),
    };
    store.set('barAnchor', anchor);
  }
}

async function place() {
  await ensureAnchor();
  lastW = 0; await fit();
}

$('cat').onclick = toggleNotes;
// Scroll on the bar: up adds a minute, down takes one away (down to zero means time's up).
let wheel = 0;
$('bar').addEventListener('wheel', (e) => {
  wheel += e.deltaY;
  const steps = Math.trunc(wheel / 100);
  if (steps) { wheel -= steps * 100; set(nudge(state, -steps)); }
}, { passive: true });

// Double-click the bar to pick a task; press and drag it to move it.
$('bar').addEventListener('dblclick', (e) => {
  if ((e.target as HTMLElement).closest('button')) return;
  if (anchor) emit('picker-open', anchor);
});
let press: { x: number; y: number } | null = null;
$('bar').addEventListener('mousedown', (e) => { if (e.button === 0 && !(e.target as HTMLElement).closest('button')) press = { x: e.screenX, y: e.screenY }; });
window.addEventListener('mouseup', () => (press = null));
window.addEventListener('mousemove', (e) => {
  if (press && Math.hypot(e.screenX - press.x, e.screenY - press.y) > 4) { press = null; win.startDragging().catch(() => {}); }
});
// Click the time to change the timer length (it becomes the default) or pause; one click picks.
let defaultMin = store.get('defaultMinutes', 25);
function showPresets(open: boolean) {
  $('bar').hidden = open; $('presets').hidden = !open;
  document.querySelectorAll<HTMLButtonElement>('[data-min]').forEach((b) => b.setAttribute('aria-pressed', String(Number(b.dataset.min) === defaultMin)));
  $('pause').textContent = state.endsAt === null && state.task ? 'Resume' : 'Pause';
  ($('pause') as HTMLButtonElement).disabled = !state.task;
  lastW = 0; fit();
}
$('clock').onclick = () => showPresets(true);
$('closepresets').onclick = () => showPresets(false);
$('pause').onclick = () => { set(toggle(state)); showPresets(false); };
document.querySelectorAll<HTMLButtonElement>('[data-min]').forEach((b) => (b.onclick = () => {
  defaultMin = Number(b.dataset.min); store.set('defaultMinutes', defaultMin);
  if (state.task) set(start(state.task, state.project, defaultMin));
  showPresets(false);
}));
document.addEventListener('keydown', (e) => { if (e.key === 'Escape') showPresets(false); });
win.onMoved(async ({ payload }) => {
  if (fitting || native) return;
  const size = await win.outerSize();
  anchor = { right: payload.x + size.width, y: payload.y };
  store.set('barAnchor', anchor);
});

listen<{ task: string; project: string; minutes: number | null }>('start-task', ({ payload }) => set(start(payload.task, payload.project, payload.minutes ?? defaultMin)));
listen<{ action: 'stop' | 'toggle' }>('timer-control', ({ payload }) => set(payload.action === 'stop' ? idle() : toggle(state)));
listen('timer-ask', () => emit('timer-state', state));

// The web bar: this window shows and ticks.
let webTimers: number[] = [];
async function goWeb() {
  native = false;
  if (webTimers.length) return;
  // Windows lifts the taskbar above other always-on-top windows now and then; quietly take the spot back.
  webTimers = [window.setInterval(() => win.setAlwaysOnTop(true).catch(() => {}), 3000), window.setInterval(render, 250)];
  await win.show();
  await place().catch(() => {});
  render();
}

// The native bar: it draws and ticks; this hidden window answers its clicks.
async function goNative() {
  native = true;
  webTimers.forEach(clearInterval); webTimers = [];
  await win.hide().catch(() => {});
  await ensureAnchor().catch(() => {});
  if (anchor) await invoke('nbar_send', { msg: { t: 'anchor', ...anchor } }).catch(() => {});
  pushNative();
}

type NativeMsg = { t: string; min?: number; right?: number; y?: number };
listen<NativeMsg>('nbar', async ({ payload: m }) => {
  switch (m.t) {
    case 'ready': return goNative();
    case 'gone': return goWeb();
    case 'again':
      if (native) { const notes = await WebviewWindow.getByLabel('notes'); await notes?.show(); await notes?.unminimize(); await notes?.setFocus(); }
      else { await win.show(); await win.unminimize(); await win.setAlwaysOnTop(true); }
      return;
    case 'cat': case 'lists': return toggleNotes();
    case 'pick': if (anchor) emit('picker-open', anchor); return;
    case 'pause': return set(toggle(state));
    case 'nudge': return set(nudge(state, m.min ?? 0));
    case 'preset':
      defaultMin = m.min ?? defaultMin; store.set('defaultMinutes', defaultMin);
      if (state.task) set(start(state.task, state.project, defaultMin)); else pushNative();
      return;
    case 'moved':
      if (typeof m.right === 'number' && typeof m.y === 'number') { anchor = { right: m.right, y: m.y }; store.set('barAnchor', anchor); }
      return;
    case 'up':
      // The native bar already played the chime.
      emit('timer-up', { task: state.task, project: state.project });
      if (anchor) emit('ask-open', { task: state.task, project: state.project, minutes: state.totalMs / 60_000, anchor });
      return;
  }
}).then(async () => {
  const mode = await invoke<string>('nbar_mode').catch(() => 'web');
  if (mode === 'ready') return goNative();
  if (mode === 'starting') { setTimeout(() => { if (!native) goWeb(); }, 8000); return; }
  return goWeb();
});
