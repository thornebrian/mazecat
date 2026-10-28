// Settings shared by Maze Cat's windows (same origin, same storage).
export function defaultMinutes(): number {
  try { const v = Number(JSON.parse(localStorage.getItem('defaultMinutes') ?? '25')); return v > 0 ? v : 25; } catch { return 25; }
}
