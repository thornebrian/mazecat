// The quick picker: pops up over the bar. Double-click a task to start it; the picker closes by itself.
import { getCurrentWindow, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
import { emit, listen } from '@tauri-apps/api/event';
import { parseProject, tagMinutes, Section } from './lists';
import { listProjects, readProject, moveTo, isReadOnly } from './source';
import { defaultMinutes } from './prefs';

const win = getCurrentWindow();
const $ = (id: string) => document.getElementById(id)!;
const remember = {
  get(k: string) { try { return localStorage.getItem(k) ?? ''; } catch { return ''; } },
  set(k: string, v: string) { try { localStorage.setItem(k, v); } catch { /* fine */ } },
};
let project = remember.get('pickerProject');
let running = { task: '', project: '' };
const W = 320, H = 380;

async function show(anchor: { right: number; y: number }) {
  const scale = await win.scaleFactor();
  await win.setSize(new LogicalSize(W, H));
  await win.setPosition(new PhysicalPosition(anchor.right - Math.round(W * scale), anchor.y - Math.round((H + 6) * scale)));
  await load(true);
  await win.show(); await win.setFocus();
  (document.querySelector('.pick') as HTMLButtonElement | null)?.focus();
}

async function load(refresh = false) {
  let names: string[] = [], note = '';
  try { ({ names, note } = await listProjects(refresh)); } catch (e) { names = []; note = String(e); }
  if (!names.includes(project)) project = names.includes(running.project) ? running.project : names[0] ?? '';
  const nav = $('projects'); nav.innerHTML = '';
  for (const n of names) {
    const b = document.createElement('button'); b.type = 'button'; b.textContent = n;
    b.setAttribute('aria-pressed', String(n === project));
    b.onclick = async () => { project = n; remember.set('pickerProject', n); await load(); };
    nav.appendChild(b);
  }
  nav.querySelector('[aria-pressed="true"]')?.scrollIntoView({ inline: 'nearest' });
  const text = project ? await readProject(project) : '';
  const p = parseProject(project, text);
  const list = $('list'); list.innerHTML = '';
  $('foot').textContent = note || (isReadOnly() ? 'Jira Sync · double-click to start · ✓ done in Jira · Esc' : 'Double-click to start · ✓ done · Esc to close');
  if (!p.ok) { list.innerHTML = `<div class="msg bad"></div>`; list.firstElementChild!.textContent = p.message; return; }
  const sections: Section[] = ['WIP', 'TODO'];
  let any = false;
  for (const s of sections) {
    if (!p.items[s].length) continue;
    any = true;
    const h = document.createElement('h2'); h.textContent = s === 'WIP' ? 'In progress' : 'To do'; list.appendChild(h);
    for (const task of p.items[s]) list.appendChild(row(task, text));
  }
  if (!any) list.innerHTML = `<div class="empty">${isReadOnly() ? 'Nothing assigned to you here.' : 'Nothing to do here. Click the cat to add tasks.'}</div>`;
}

function row(task: string, text: string) {
  const el = document.createElement('div'); el.className = 'item'; el.setAttribute('role', 'listitem');
  if (running.task === task && running.project === project) el.classList.add('now');
  const pick = document.createElement('button'); pick.type = 'button'; pick.className = 'pick';
  pick.textContent = task;
  const m = tagMinutes(task); const mins = document.createElement('span'); mins.className = 'mins'; mins.textContent = `${m ?? defaultMinutes()}m`; pick.appendChild(mins);
  pick.ondblclick = () => start(task, text);
  pick.onkeydown = (e) => { if (e.key === 'Enter') start(task, text); };
  const done = document.createElement('button'); done.type = 'button'; done.className = 'done'; done.textContent = '✓';
  done.setAttribute('aria-label', `Mark "${task}" done`);
  done.onclick = () => finish(task, text);
  el.append(pick, done);
  return el;
}

async function start(task: string, text: string) {
  await win.hide();
  await emit('start-task', { task, project, minutes: tagMinutes(task) });
  try { await moveTo(project, task, 'WIP', text); } catch (e) { console.warn(e); }
  await emit('lists-changed', { project });
}

async function finish(task: string, text: string) {
  let note = '';
  try { note = await moveTo(project, task, 'DONE', text); } catch (e) { note = String(e); }
  if (running.task === task && running.project === project) await emit('timer-control', { action: 'stop' });
  await emit('lists-changed', { project });
  await load();
  if (note) $('foot').textContent = note;
}

listen<{ right: number; y: number }>('picker-open', ({ payload }) => show(payload));
listen<{ task: string; project: string }>('timer-state', ({ payload }) => { running = { task: payload.task, project: payload.project }; });
win.onFocusChanged(({ payload: focused }) => { if (!focused) win.hide(); });
document.addEventListener('keydown', (e) => { if (e.key === 'Escape') win.hide(); });
emit('timer-ask');
