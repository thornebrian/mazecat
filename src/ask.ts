// Time's up: "Yes" runs the same task again; "No" marks it done and starts the next task under TODO.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { moveTask, firstTodo, tagMinutes } from './lists';

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id)!;
const W = 250, H = 44;
let ended: { task: string; project: string; minutes: number } | null = null;

listen<{ task: string; project: string; minutes: number; anchor: { right: number; y: number } }>('ask-open', async ({ payload }) => {
  ended = payload;
  $('q').title = `${payload.task}\nYes: run it again · No: mark it done and start the next task`;
  const scale = await win.scaleFactor();
  await win.setSize(new LogicalSize(W, H));
  await win.setPosition(new PhysicalPosition(payload.anchor.right - Math.round(W * scale), payload.anchor.y - Math.round((H + 6) * scale)));
  await win.show(); await win.setFocus(); $('yes').focus();
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
  let text = await invoke<string>('read_project', { name: project });
  text = moveTask(text, task, 'DONE');
  const next = firstTodo(project, text);
  if (next) {
    text = moveTask(text, next, 'WIP');
    await emit('start-task', { task: next, project, minutes: tagMinutes(next) ?? 25 });
  } else {
    await emit('timer-control', { action: 'stop' });
  }
  await invoke('write_project', { name: project, text });
  await emit('lists-changed', { project });
};

document.addEventListener('keydown', (e) => { if (e.key === 'Escape') win.hide(); });
