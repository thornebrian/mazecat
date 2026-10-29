// The lists window: edit a project's YAML (Local lists) or read your Jira issues (Jira Sync),
// pick a task, start the timer, mark it done.
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { catSvg } from './cat';
import { parseProject, taskAtLine, tagMinutes, moveTask, newProject, Section } from './lists';
import { defaultMinutes, source, setSource, Source } from './prefs';
import { listProjects, readProject, moveTo, isReadOnly } from './source';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const editor = $<HTMLTextAreaElement>('editor'), select = $<HTMLSelectElement>('project'), mode = $<HTMLSelectElement>('source');
let current = '';                       // open list name (a file name without .yaml, or a Jira project)
let saveTimer: number | undefined;
let picked: { task: string; section: Section | null } | null = null;
let running: { task: string; project: string } | null = null;

$('logo').innerHTML = catSvg('calm');

function applyMode() {
  const ro = isReadOnly();
  mode.value = source();
  editor.readOnly = ro;
  $('ro').hidden = !ro;
  $('newlist').hidden = ro;
  $('jirasettings').hidden = !ro;
  $('refresh').hidden = !ro;
  editor.setAttribute('aria-label', ro ? 'Your Jira issues (read-only)' : 'Task list (YAML)');
}

async function loadList(name?: string, refresh = false) {
  applyMode();
  let names: string[] = [], note = '', warn = false;
  try { ({ names, note, warn } = await listProjects(refresh)); } catch (e) { note = String(e); warn = true; }
  select.innerHTML = '';
  for (const n of names) select.add(new Option(n, n));
  current = name && names.includes(name) ? name : names[0] ?? '';
  select.value = current;
  editor.value = current ? await readProject(current) : '';
  check(); pick();
  if (note) say(note, warn);
}

function say(text: string, bad = false) { const st = $('status'); st.textContent = text; st.classList.toggle('bad', bad); }

function check() {
  const p = parseProject(current, editor.value);
  say(p.ok ? (p.message || `${p.items.WIP.length} in progress · ${p.items.TODO.length} to do · ${p.items.DONE.length} done`) : p.message, !p.ok);
  return p.ok;
}

function save() {
  if (isReadOnly()) return;
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => { if (current) invoke('write_project', { name: current, text: editor.value }).catch((e) => say(`Couldn't save: ${e}`, true)); }, 500);
}

async function saveNow() {
  clearTimeout(saveTimer);
  if (current && !isReadOnly()) await invoke('write_project', { name: current, text: editor.value });
}

function caretLine() { return editor.value.slice(0, editor.selectionStart).split('\n').length - 1; }

function pick() {
  picked = taskAtLine(editor.value, caretLine());
  $('picked').textContent = picked ? picked.task : 'Click a task line above';
  const mins = picked ? tagMinutes(picked.task) : null;
  $('go').textContent = `Start · ${mins ?? defaultMinutes()} min`;
}

async function startTask(minutes?: number) {
  if (!picked) { say('Put the cursor on a task line (one that starts with "- ") first.'); return; }
  const task = picked.task, mins = minutes ?? tagMinutes(task);
  await emit('start-task', { task, project: current, minutes: mins });
  if (picked.section === 'WIP') return;
  if (isReadOnly()) {
    try { await moveTo(current, task, 'WIP'); editor.value = await readProject(current); check(); }
    catch (e) { say(String(e), true); }
  } else {
    editor.value = moveTask(editor.value, task, 'WIP'); save(); check();
  }
}

async function markDone() {
  const task = picked?.task ?? running?.task;
  if (!task) return;
  if (isReadOnly()) {
    try { const note = await moveTo(current, task, 'DONE'); editor.value = await readProject(current); check(); pick(); if (note) say(note); }
    catch (e) { say(String(e), true); }
  } else {
    editor.value = moveTask(editor.value, task, 'DONE'); save(); check(); pick();
  }
  if (running?.task === task) await emit('timer-control', { action: 'stop' });
}

editor.addEventListener('input', () => { check(); save(); pick(); });
['click', 'keyup'].forEach((ev) => editor.addEventListener(ev, pick));
editor.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && e.ctrlKey) { e.preventDefault(); startTask(); }
  if (e.key === 'Tab' && !isReadOnly()) { e.preventDefault(); editor.setRangeText('  ', editor.selectionStart, editor.selectionEnd, 'end'); }
});
select.addEventListener('change', async () => { await saveNow(); await loadList(select.value); });
mode.addEventListener('change', async () => {
  await saveNow();
  setSource(mode.value as Source);
  await emit('lists-changed', { project: '' });
  if (isReadOnly() && !(await invoke('jira_config'))) openSettings();
  await loadList(undefined, true);
});
$('go').onclick = () => startTask();
document.querySelectorAll<HTMLButtonElement>('[data-min]').forEach((b) => (b.onclick = () => startTask(Number(b.dataset.min))));
$('done').onclick = markDone;
$('refresh').onclick = () => loadList(current, true);
$('newlist').onclick = async () => {
  const base = 'New list';
  const names = await invoke<string[]>('list_projects');
  let name = base, i = 2;
  while (names.includes(name)) name = `${base} ${i++}`;
  await invoke('write_project', { name, text: newProject(name) });
  await loadList(name);
  say(`Created "${name}.yaml". Rename the file in your Notes folder to rename the list.`);
};

// Jira settings: the token goes straight to the credential store and is never shown again.
const dlg = $<HTMLDialogElement>('jiradlg');
async function openSettings() {
  const cfg = await invoke<{ site: string; email: string; jql: string } | null>('jira_config');
  $<HTMLInputElement>('jsite').value = cfg?.site ?? '';
  $<HTMLInputElement>('jemail').value = cfg?.email ?? '';
  $<HTMLInputElement>('jjql').value = cfg?.jql ?? '';
  $<HTMLInputElement>('jtoken').value = '';
  $('jstatus').textContent = '';
  dlg.showModal();
}
$('jirasettings').onclick = openSettings;
$('jiraform').addEventListener('submit', async (e) => {
  if ((e as SubmitEvent).submitter?.getAttribute('value') !== 'save') return;
  e.preventDefault();
  const token = $<HTMLInputElement>('jtoken').value;
  try {
    await invoke('jira_save', { site: $<HTMLInputElement>('jsite').value, email: $<HTMLInputElement>('jemail').value, jql: $<HTMLInputElement>('jjql').value, token: token || null });
    $<HTMLInputElement>('jtoken').value = '';
    const { names, note, warn } = await listProjects(true);
    if (warn) { $('jstatus').textContent = note; return; }
    dlg.close();
    await loadList(undefined, false);
    say(`Connected: ${names.length} Jira project${names.length === 1 ? '' : 's'}.`);
  } catch (err) { $('jstatus').textContent = String(err); }
});

listen<{ project: string }>('lists-changed', async ({ payload }) => {
  if ((payload.project === current || !payload.project) && document.activeElement !== editor) await loadList(current);
});
listen<{ task: string; project: string; totalMs: number }>('timer-state', ({ payload }) => { running = payload.task ? payload : null; });
listen<{ task: string; project: string }>('timer-up', ({ payload }) => {
  if (payload.project === current) say(`Time's up for "${payload.task}". Mark it done, or start it again.`);
});

// In Jira Sync, check Jira for changes every minute while the window is open.
setInterval(() => { if (isReadOnly() && !document.hidden && !dlg.open) loadList(current, true).catch(() => {}); }, 60_000);

// Closing the window only hides it; the cat on the bar brings it back.
getCurrentWindow().onCloseRequested(async (e) => { e.preventDefault(); await saveNow(); await getCurrentWindow().hide(); });

emit('timer-ask');
loadList().catch((e) => say(`Couldn't open your lists: ${e}`, true));
