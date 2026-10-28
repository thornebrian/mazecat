// The bar: current task and countdown, floating just above the taskbar.
import { getCurrentWindow, PhysicalPosition, primaryMonitor } from '@tauri-apps/api/window';
import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { listen, emit } from '@tauri-apps/api/event';
import { catSvg } from './cat';
import { TimerState, idle, start, remaining, toggle, format } from './timer';

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id)!;
const store = {
  get<T>(k: string, d: T): T { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch { return d; } },
  set(k: string, v: unknown) { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* storage off: fine */ } },
};

let state: TimerState = store.get('timer', idle());
let rang = false;

function render() {
  const left = remaining(state);
  const task = $('task'), clock = $('clock');
  const up = !!state.task && state.totalMs > 0 && left === 0;
  task.textContent = state.task ? (up ? `Time's up · ${state.task}` : state.task) : 'Pick a task in your lists';
  task.title = state.task ? `${state.project} · ${state.task}` : '';
  task.classList.toggle('idle', !state.task);
  clock.textContent = state.task ? format(left) : '--:--';
  clock.className = 'clock' + (up ? ' up' : state.endsAt !== null ? ' running' : state.task ? ' paused' : '');
  $('cat').innerHTML = catSvg(up ? 'awake' : 'calm');
  if (up && !rang) { rang = true; chime(); emit('timer-up', { task: state.task, project: state.project }); }
  if (!up) rang = false;
}

function set(s: TimerState) { state = s; store.set('timer', s); render(); emit('timer-state', s); }

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

async function place() {
  const saved = store.get<{ x: number; y: number } | null>('barPos', null);
  if (saved) return win.setPosition(new PhysicalPosition(saved.x, saved.y));
  const m = await primaryMonitor();
  if (!m) return;
  const size = await win.outerSize();
  // Bottom right, over the taskbar, leaving room for the clock and tray icons.
  const x = m.position.x + m.size.width - size.width - Math.round(260 * m.scaleFactor);
  const y = m.position.y + m.size.height - size.height - Math.round(4 * m.scaleFactor);
  await win.setPosition(new PhysicalPosition(x, y));
}

$('cat').onclick = toggleNotes;
$('clock').onclick = () => set(toggle(state));
win.onMoved(({ payload }) => store.set('barPos', { x: payload.x, y: payload.y }));

listen<{ task: string; project: string; minutes: number }>('start-task', ({ payload }) => set(start(payload.task, payload.project, payload.minutes)));
listen<{ action: 'stop' | 'toggle' }>('timer-control', ({ payload }) => set(payload.action === 'stop' ? idle() : toggle(state)));
listen('timer-ask', () => emit('timer-state', state));

// Windows lifts the taskbar above other always-on-top windows now and then; quietly take the spot back.
setInterval(() => win.setAlwaysOnTop(true).catch(() => {}), 3000);
setInterval(render, 250);
place().catch(() => {});
render();
