// Jira Sync: turn Jira issues into Maze Cat lists. Pure (no Tauri), so it's easy to test.
// In Jira Sync mode the lists are read-only; the only change Maze Cat makes is a task's state.
import YAML from 'yaml';
import type { Section } from './lists';

export interface Issue {
  key: string;
  summary: string;
  project_key: string;
  project_name: string;
  /** Jira's status category: "new", "indeterminate" or "done". */
  category: string;
  labels: string[];
  /** Original estimate in seconds, when set. */
  estimate: number | null;
  /** In an active sprint; otherwise it waits in the backlog (PLANNED). */
  in_sprint: boolean;
}

export type Category = 'new' | 'indeterminate' | 'done';
export const CATEGORY: Record<Section, Category> = { PLANNED: 'new', TODO: 'new', WIP: 'indeterminate', DONE: 'done' };

/** Backlog → PLANNED; in the sprint: to do → TODO, in progress → WIP, done → DONE. */
export function sectionOf(i: Issue): Section {
  if (i.category === 'done') return 'DONE';
  if (i.category === 'indeterminate') return 'WIP';
  return i.in_sprint ? 'TODO' : 'PLANNED';
}

/** The list name for an issue's project, e.g. "MIQR · miQR". */
export const projectLabel = (i: Issue) => `${i.project_key} · ${i.project_name}`;

/**
 * One task line: the summary, the issue key, then a timer tag so the timer still works.
 * A label easy / mid / hard wins; otherwise the original estimate becomes a time tag.
 * The key sits before the tag because tags are read from the end of the line.
 */
export function issueLine(i: Issue): string {
  const level = i.labels.map((l) => l.toLowerCase()).find((l) => l === 'easy' || l === 'mid' || l === 'hard');
  const tag = level ? `(${level})` : i.estimate ? `(${Math.max(1, Math.round(i.estimate / 60))}m)` : '';
  return `${i.summary.replace(/\s+/g, ' ').trim()} [${i.key}]${tag ? ` ${tag}` : ''}`;
}

/** Project list names, in the order Jira returned them. */
export function jiraProjects(issues: Issue[]): string[] {
  return [...new Set(issues.map(projectLabel))];
}

/** A project as the same YAML the local lists use, so every window reads it the same way. */
export function jiraYaml(label: string, issues: Issue[]): string {
  const body: Record<Section, string[]> = { WIP: [], TODO: [], PLANNED: [], DONE: [] };
  for (const i of issues) if (projectLabel(i) === label) body[sectionOf(i)].push(issueLine(i));
  return YAML.stringify({ [label]: body });
}

/** The Jira key in a task line, e.g. "Fix the footer [MIQR-12] (easy)" → "MIQR-12". */
export function keyOf(task: string): string | null {
  return /\[([A-Z][A-Z0-9_]*-\d+)\]/.exec(task)?.[1] ?? null;
}
