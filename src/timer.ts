// A countdown that survives pauses and restarts. Pure, so it's easy to test.
export interface TimerState {
  task: string;
  project: string;
  totalMs: number;
  /** Set while running: when the countdown reaches zero (epoch ms). */
  endsAt: number | null;
  /** Set while paused: how much was left. */
  leftMs: number;
}

export const idle = (): TimerState => ({ task: '', project: '', totalMs: 0, endsAt: null, leftMs: 0 });

export function start(task: string, project: string, minutes: number, now = Date.now()): TimerState {
  const totalMs = Math.round(minutes * 60_000);
  return { task, project, totalMs, endsAt: now + totalMs, leftMs: totalMs };
}

export function remaining(s: TimerState, now = Date.now()): number {
  return s.endsAt === null ? s.leftMs : Math.max(0, s.endsAt - now);
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
