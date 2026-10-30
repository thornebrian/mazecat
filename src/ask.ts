// Time's up: "Yes" runs the same task again; "No" marks it done and starts the next task under TODO.
import { getCurrentWindow, PhysicalPosition, PhysicalSize } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { firstTodo, tagMinutes } from './lists';
import { readProject, moveTo } from './source';

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id)!;
const H = 44;
// Wide enough for its words, whatever the text size: measured, not guessed, in real
// screen pixels (devicePixelRatio includes Windows' text size and display scaling).
function sizePx() {
  const pop = document.querySelector('.pop') as HTMLElement;
  pop.style.right = 'auto'; pop.style.width = 'max-content';
  const w = pop.getBoundingClientRect().width;
  pop.style.right = ''; pop.style.width = '';
  const r = window.devicePixelRatio || 1;
  return { w: Math.ceil((w + 2) * r), h: Math.ceil(H * r) };
}
let ended: { task: string; project: string; minutes: number } | null = null;

listen<{ task: string; project: string; minutes: number; anchor: { right: number; y: number } }>('ask-open', async ({ payload }) => {
  ended = payload;
  $('q').title = `${payload.task}\nYes: run it again · No: mark it done and start the next task`;
  const { w, h } = sizePx();
  const gap = Math.round(6 * (window.devicePixelRatio || 1));
  await win.setSize(new PhysicalSize(w, h));
  await win.setPosition(new PhysicalPosition(payload.anchor.right - w, payload.anchor.y - h - gap));
  // Shown without taking focus, so your typing stays where it was and can't press Yes or
  // No by accident. It stays up until you click one.
  await win.show();
});

$('yes').onclick = async () => {
  if (!ended) return;
  await win.hide();
  await emit('start-task', { task: ended.task, project: ended.project, minutes: ended.minutes });
};

$('no').onclick = async () => {
  if (!ended) return;
  await win.hide();
  const { task, project } = ended;
  try { await moveTo(project, task, 'DONE'); } catch (e) { console.warn(e); }
  const next = firstTodo(project, await readProject(project));
  if (next) {
    await emit('start-task', { task: next, project, minutes: tagMinutes(next) });
    try { await moveTo(project, next, 'WIP'); } catch (e) { console.warn(e); }
  } else {
    await emit('timer-control', { action: 'stop' });
  }
  await emit('lists-changed', { project });
};

document.addEventListener('keydown', (e) => { if (e.key === 'Escape') win.hide(); });
