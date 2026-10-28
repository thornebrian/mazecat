// Maze Cat's mark: an original, simple cat head. Two moods: calm (timer running or idle) and awake (time's up).
export function catSvg(mood: 'calm' | 'awake' = 'calm'): string {
  const eyes = mood === 'awake'
    ? '<circle cx="9.2" cy="13" r="1.6"/><circle cx="14.8" cy="13" r="1.6"/>'
    : '<path d="M7.8 13.2q1.4-1.2 2.8 0M13.4 13.2q1.4-1.2 2.8 0" fill="none" stroke="var(--cat-face)" stroke-width="1.3" stroke-linecap="round"/>';
  return `<svg viewBox="0 0 24 24" aria-hidden="true" class="cat ${mood}">
    <path d="M4 20V7.5L7.6 3l3 4.2h2.8l3-4.2L20 7.5V20a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z" fill="var(--cat)"/>
    <g fill="var(--cat-face)">${eyes}</g>
    <path d="M11 16.4h2l-1 1.1Z" fill="var(--cat-face)"/>
  </svg>`;
}
