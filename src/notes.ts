// The lists window: edit a project's YAML, pick a task, start the timer, mark it done.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { catSvg } from './cat';
import { parseProject, taskAtLine, tagMinutes, moveTask, newProject, Section } from './lists';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const editor = $<HTMLTextAreaElement>('editor'), select = $<HTMLSelectElement>('project');
let current = '';                       // open file name, without .yaml
let saveTimer: number | undefined;
let picked: { task: string; section: Section | null } | null = null;
let running: { task: string; project: string } | null = null;

$('logo').innerHTML = catSvg('calm');

async function loadList(name?: string) {
  const names = await invoke<string[]>('list_projects');
  select.innerHTML = '';
  for (const n of names) select.add(new Option(n, n));
  current = name && names.includes(name) ? name : names[0] ?? '';
  select.value = current;
  editor.value = current ? await invoke<string>('read_project', { name: current }) : '';
  check(); pick();
}

function check() {
  const p = parseProject(current, editor.value);
  const st = $('status');
  st.textContent = p.ok ? (p.message || `${p.items.WIP.length} in progress · ${p.items.TODO.length} to do · ${p.items.DONE.length} done`) : p.message;
  st.classList.toggle('bad', !p.ok);
  return p.ok;
}

function save() {
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => { if (current) invoke('write_project', { name: current, text: editor.value }).catch((e) => ($('status').textContent = `Couldn't save: ${e}`)); }, 500);
}

function caretLine() { return editor.value.slice(0, editor.selectionStart).split('\n').length - 1; }

function pick() {
  picked = taskAtLine(editor.value, caretLine());
  $('picked').textContent = picked ? picked.task : 'Click a task line above';
  const mins = picked ? tagMinutes(picked.task) : null;
  $('go').textContent = mins ? `Start · ${mins} min` : 'Start · 25 min';
}

async function startTask(minutes?: number) {
  if (!picked) { $('status').textContent = 'Put the cursor on a task line (one that starts with "- ") first.'; return; }
  const mins = minutes ?? tagMinutes(picked.task) ?? 25;
  if (picked.section !== 'WIP') { editor.value = moveTask(editor.value, picked.task, 'WIP'); save(); check(); }
  await emit('start-task', { task: picked.task, project: current, minutes: mins });
}

async function markDone() {
  const task = picked?.task ?? running?.task;
  if (!task) return;
  editor.value = moveTask(editor.value, task, 'DONE'); save(); check(); pick();
  if (running?.task === task) await emit('timer-control', { action: 'stop' });
}

editor.addEventListener('input', () => { check(); save(); pick(); });
['click', 'keyup'].forEach((ev) => editor.addEventListener(ev, pick));
editor.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && e.ctrlKey) { e.preventDefault(); startTask(); }
  if (e.key === 'Tab') { e.preventDefault(); editor.setRangeText('  ', editor.selectionStart, editor.selectionEnd, 'end'); }
});
select.addEventListener('change', async () => { clearTimeout(saveTimer); if (current) await invoke('write_project', { name: current, text: editor.value }); await loadList(select.value); });
$('go').onclick = () => startTask();
document.querySelectorAll<HTMLButtonElement>('[data-min]').forEach((b) => (b.onclick = () => startTask(Number(b.dataset.min))));
$('done').onclick = markDone;
$('newlist').onclick = async () => {
  const base = 'New list';
  const names = await invoke<string[]>('list_projects');
  let name = base, i = 2;
  while (names.includes(name)) name = `${base} ${i++}`;
  await invoke('write_project', { name, text: newProject(name) });
  await loadList(name);
  $('status').textContent = `Created "${name}.yaml". Rename the file in your Notes folder to rename the list.`;
};

listen<{ project: string }>('lists-changed', async ({ payload }) => {
  if (payload.project === current && document.activeElement !== editor) { editor.value = await invoke<string>('read_project', { name: current }); check(); pick(); }
});
listen<{ task: string; project: string; totalMs: number }>('timer-state', ({ payload }) => { running = payload.task ? payload : null; });
listen<{ task: string; project: string }>('timer-up', ({ payload }) => {
  if (payload.project === current) $('status').textContent = `Time's up for "${payload.task}". Mark it done, or start it again.`;
});

// Closing the window only hides it; the cat on the bar brings it back.
getCurrentWindow().onCloseRequested(async (e) => { e.preventDefault(); clearTimeout(saveTimer); if (current) await invoke('write_project', { name: current, text: editor.value }); await getCurrentWindow().hide(); });

emit('timer-ask');
loadList().catch((e) => ($('status').textContent = `Couldn't open your lists: ${e}`));
