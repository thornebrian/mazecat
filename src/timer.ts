// A countdown that survives pauses and restarts. Pure, so it's easy to test.
export interface TimerState {
  task: string;
  project: string;
  totalMs: number;
  /** Set while running: when the countdown reaches zero (epoch ms). */
  endsAt: number | null;
  /** Set while paused: how much was left. */
  leftMs: number;
  /** When this run began (epoch ms). A new run gets a new value; pause and resume keep it. */
  startedAt?: number;
}

export const idle = (): TimerState => ({ task: '', project: '', totalMs: 0, endsAt: null, leftMs: 0, startedAt: 0 });

export function start(task: string, project: string, minutes: number, now = Date.now()): TimerState {
  const totalMs = Math.round(minutes * 60_000);
  return { task, project, totalMs, endsAt: now + totalMs, leftMs: totalMs, startedAt: now };
}

export function remaining(s: TimerState, now = Date.now()): number {
  return s.endsAt === null ? s.leftMs : Math.max(0, s.endsAt - now);
}

/** Time spent on this run so far (pauses don't count). */
export function elapsedMs(s: TimerState, now = Date.now()): number {
  return s.task ? Math.max(0, s.totalMs - remaining(s, now)) : 0;
}

/** True when `next` replaces the run in `prev` (another task, a restart, or stop), so `prev`'s time is final. */
export function runEnded(prev: TimerState, next: TimerState): boolean {
  return !!prev.task && (prev.task !== next.task || prev.project !== next.project || (prev.startedAt ?? 0) !== (next.startedAt ?? 0));
}

export function toggle(s: TimerState, now = Date.now()): TimerState {
  if (!s.task || s.totalMs === 0) return s;
  if (s.endsAt !== null) return { ...s, endsAt: null, leftMs: remaining(s, now) };   // pause
  if (s.leftMs === 0) return start(s.task, s.project, s.totalMs / 60_000, now);       // time was up: go again
  return { ...s, endsAt: now + s.leftMs };                                            // resume
}

export function format(ms: number): string {
  const total = Math.ceil(ms / 1000), h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), sec = total % 60;
  const mm = String(m).padStart(h ? 2 : 1, '0'), ss = String(sec).padStart(2, '0');
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

/** Mouse wheel on the bar: add or take away minutes. Down to zero means time's up; up from
 *  time's up starts counting again. Paused stays paused. Time already spent is kept as is (it's
 *  what Jira gets), so the total becomes spent + left: scrolling up refills the sand clock. */
export function nudge(s: TimerState, minutes: number, now = Date.now()): TimerState {
  if (!s.task || s.totalMs === 0) return s;
  const spent = elapsedMs(s, now);
  const left = Math.max(0, remaining(s, now) + minutes * 60_000);
  const totalMs = Math.max(1, spent + left);
  if (s.endsAt !== null) return { ...s, totalMs, endsAt: now + left, leftMs: left };
  return { ...s, totalMs, leftMs: left };
}
