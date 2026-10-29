// Where the lists come from. Every window goes through here, so Local and Jira Sync behave the same way.
//  - Local: plain YAML files you edit freely.
//  - Jira Sync: issues from Jira, read-only; Maze Cat only changes a task's state (and logs time spent when done).
import { invoke } from '@tauri-apps/api/core';
import { moveTask, Section } from './lists';
import { Issue, jiraProjects, jiraYaml, keyOf, CATEGORY } from './jira';
import { source, takeSpentSeconds } from './prefs';

interface Snapshot { issues: Issue[]; stale: boolean; message: string; backlog_more: boolean }
let snap: Snapshot | null = null;

async function jira(refresh: boolean): Promise<Snapshot> {
  if (!snap || refresh) snap = await invoke<Snapshot>('jira_issues', { refresh });
  return snap;
}

/** List names, plus a note to show (e.g. "offline, showing the last saved lists"). */
export async function listProjects(refresh = false): Promise<{ names: string[]; note: string; warn: boolean }> {
  if (source() === 'local') return { names: await invoke<string[]>('list_projects'), note: '', warn: false };
  const s = await jira(refresh);
  const note = s.stale ? s.message
    : !s.issues.length ? 'No Jira issues assigned to you. Check the filter in Jira settings.'
    : s.backlog_more ? 'PLANNED shows the top 25 backlog items by rank; the rest stay in Jira.' : '';
  return { names: jiraProjects(s.issues), note, warn: s.stale || !s.issues.length };
}

export async function readProject(name: string): Promise<string> {
  if (source() === 'local') return invoke<string>('read_project', { name });
  return jiraYaml(name, (await jira(false)).issues);
}

/**
 * Moves a task to WIP, TODO or DONE. Returns a short note to show, or ''.
 * Local: rewrites the file. Jira: changes the issue's state, and on DONE logs the time spent.
 */
export async function moveTo(project: string, task: string, to: Section, text?: string): Promise<string> {
  if (source() === 'local') {
    const before = text ?? await invoke<string>('read_project', { name: project });
    const after = moveTask(before, task, to);
    if (after !== before) await invoke('write_project', { name: project, text: after });
    if (to === 'DONE') takeSpentSeconds(project, task); // local lists don't log time; just reset the total
    return '';
  }
  const key = keyOf(task);
  if (!key) throw new Error('This task has no Jira key.');
  const seconds = to === 'DONE' ? takeSpentSeconds(project, task) : undefined;
  const note = await invoke<string>('jira_move', { key, category: CATEGORY[to], seconds });
  snap = null; // read fresh next time
  return note;
}

export const isReadOnly = () => source() === 'jira';
