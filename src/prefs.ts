// Settings shared by Maze Cat's windows (same origin, same storage).
import { elapsedMs, TimerState } from './timer';

const read = <T>(k: string, d: T): T => { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch { return d; } };
const write = (k: string, v: unknown) => { try { localStorage.setItem(k, JSON.stringify(v)); } catch { /* storage off: fine */ } };

export function defaultMinutes(): number {
  const v = Number(read('defaultMinutes', 25)); return v > 0 ? v : 25;
}

/** Where the lists come from: plain YAML files you edit, or Jira (read-only, state changes only). */
export type Source = 'local' | 'jira';
export const source = (): Source => (read<string>('source', 'local') === 'jira' ? 'jira' : 'local');
export const setSource = (s: Source) => write('source', s);

// Time spent per task, so it can be logged in Jira when the task is done.
const spentKey = (project: string, task: string) => `${project}\u0000${task}`;

const runKey = (t: TimerState) => `${t.project}\u0000${t.task}\u0000${t.startedAt ?? 0}`;

/** True when this run's time was already counted (its task was marked done while it ran). */
export const alreadyLogged = (t: TimerState) => read('loggedRun', '') === runKey(t);

/** Adds a finished timer run to its task's total. */
export function addSpent(project: string, task: string, ms: number) {
  if (!task || ms <= 0) return;
  const all = read<Record<string, number>>('spent', {});
  all[spentKey(project, task)] = (all[spentKey(project, task)] ?? 0) + ms;
  write('spent', all);
}

/** Total time on a task so far, including the run going on now, in seconds. Clears the total. */
export function takeSpentSeconds(project: string, task: string): number {
  const all = read<Record<string, number>>('spent', {});
  let ms = all[spentKey(project, task)] ?? 0;
  const t = read<TimerState | null>('timer', null);
  if (t && t.task === task && t.project === project) { ms += elapsedMs(t); write('loggedRun', runKey(t)); }
  delete all[spentKey(project, task)];
  write('spent', all);
  return Math.round(ms / 1000);
}
