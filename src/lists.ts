// Task lists: one project per .yaml file, sections WIP / TODO / DONE, one "- item" per line.
// Kept free of Tauri so it can be tested on its own (see tests/).
import YAML from 'yaml';

export type Section = 'WIP' | 'TODO' | 'DONE';
export const SECTIONS: Section[] = ['WIP', 'TODO', 'DONE'];

export interface Parsed {
  ok: boolean;
  /** Plain-language problem, shown under the editor. Empty when ok. */
  message: string;
  items: Record<Section, string[]>;
}

const empty = (): Record<Section, string[]> => ({ WIP: [], TODO: [], DONE: [] });

/** Read a project file. Never throws: problems come back as a friendly message. */
export function parseProject(fileName: string, text: string): Parsed {
  let doc: unknown;
  try {
    doc = YAML.parse(text);
  } catch (e) {
    const line = (e as { linePos?: { line: number }[] }).linePos?.[0]?.line;
    return { ok: false, message: `This list isn't valid YAML${line ? ` (line ${line})` : ''}. Check for a stray colon, quote or # in a task.`, items: empty() };
  }
  if (doc == null) return { ok: true, message: '', items: empty() };
  if (typeof doc !== 'object' || Array.isArray(doc)) {
    return { ok: false, message: `Start the file with the project name, like "${fileName}:".`, items: empty() };
  }
  const keys = Object.keys(doc as object);
  if (keys.length > 1) {
    return { ok: false, message: `This file holds ${keys.length} projects (${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '…' : ''}). Maze Cat reads one project per file; split them into separate files.`, items: empty() };
  }
  const body = (doc as Record<string, unknown>)[keys[0]];
  const items = empty();
  if (body && typeof body === 'object' && !Array.isArray(body)) {
    for (const s of SECTIONS) {
      const list = (body as Record<string, unknown>)[s];
      if (Array.isArray(list)) items[s] = list.map(asText).filter(Boolean);
    }
  }
  const note = keys[0] !== fileName ? `Heads-up: the project is named "${keys[0]}" but the file is "${fileName}".` : '';
  return { ok: true, message: note, items };
}

/** A task line YAML read as something else ("Call Ana: 3 pm" becomes a map) goes back to plain text. */
function asText(v: unknown): string {
  if (v == null) return '';
  if (typeof v === 'object' && !Array.isArray(v)) {
    return Object.entries(v as Record<string, unknown>).map(([k, x]) => `${k}: ${x ?? ''}`.trim()).join(', ');
  }
  return String(v).trim();
}

/**
 * Minutes set by a task's tags at the end of the line.
 * A time tag wins: (15m), (15 min), (1h), (1.5h), (1h30). Otherwise a difficulty tag: (easy) 10, (mid) 25, (hard) 60.
 */
export function tagMinutes(task: string): number | null {
  const tags = (/(\s*\([^()]*\))+\s*$/.exec(task)?.[0] ?? '').match(/\(([^()]*)\)/g) ?? [];
  let level: number | null = null;
  for (const raw of tags) {
    for (const part of raw.slice(1, -1).split(/[,;]/).map((x) => x.trim().toLowerCase())) {
      const t = /^(\d+(?:\.\d+)?)\s*(h|hr|hrs|hours?)(?:\s*(\d+)\s*(?:m|min|mins)?)?$|^(\d+)\s*(m|min|mins|minutes?)$/.exec(part);
      if (t) return t[1] ? Math.round(Number(t[1]) * 60 + Number(t[3] ?? 0)) : Number(t[4]);
      const d = ({ easy: 10, mid: 25, hard: 60 } as Record<string, number>)[part];
      if (d && level === null) level = d;
    }
  }
  return level;
}

/** The task on a given line of the editor, and its section, or null when the line isn't a task. */
export function taskAtLine(text: string, lineNo: number): { task: string; section: Section | null } | null {
  const lines = text.split(/\r?\n/);
  const m = /^\s*-\s+(.*\S)\s*$/.exec(lines[lineNo] ?? '');
  if (!m) return null;
  let section: Section | null = null;
  for (let i = lineNo; i >= 0; i--) {
    const h = /^\s*(WIP|TODO|DONE)\s*:\s*$/.exec(lines[i]);
    if (h) { section = h[1] as Section; break; }
  }
  return { task: unquote(m[1]), section };
}

function unquote(s: string): string {
  if (/^'.*'$/.test(s)) return s.slice(1, -1).replace(/''/g, "'");
  if (/^".*"$/.test(s)) { try { return JSON.parse(s); } catch { return s.slice(1, -1); } }
  return s;
}

/** Move a task's line to another section, keeping every other line exactly as written. */
export function moveTask(text: string, task: string, to: Section): string {
  const nl = text.includes('\r\n') ? '\r\n' : '\n';
  const lines = text.split(/\r?\n/);
  const at = lines.findIndex((l, i) => taskAtLine(text, i)?.task === task && /^\s*-\s/.test(l));
  if (at < 0) return text;
  const [line] = lines.splice(at, 1);
  let head = lines.findIndex((l) => new RegExp(`^\\s*${to}\\s*:\\s*$`).test(l));
  if (head < 0) { lines.push(`  ${to}:`); head = lines.length - 1; }
  // Insert right under the section header, so the newest moves sit on top.
  lines.splice(head + 1, 0, line);
  return lines.join(nl);
}

/** A starter list for a new project file. */
export function newProject(name: string): string {
  return `${name}:\n  WIP:\n  TODO:\n  - First task (easy)\n  DONE:\n`;
}

/** The first task under TODO, or null when there is none. */
export function firstTodo(fileName: string, text: string): string | null {
  const p = parseProject(fileName, text);
  return p.ok ? p.items.TODO[0] ?? null : null;
}
