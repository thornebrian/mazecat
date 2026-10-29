import { test } from 'node:test';
import assert from 'node:assert/strict';
import { issueLine, jiraProjects, jiraYaml, keyOf, sectionOf, CATEGORY } from '../src/jira.ts';
import { parseProject, tagMinutes, taskAtLine } from '../src/lists.ts';
import { start, toggle, elapsedMs, runEnded, idle } from '../src/timer.ts';

const issue = (o) => ({ key: 'MIQR-12', summary: 'Fix the footer', project_key: 'MIQR', project_name: 'miQR', category: 'new', labels: [], estimate: null, in_sprint: true, ...o });

test('an issue becomes a task line with its key, and the timer still reads its tag', () => {
  const line = issueLine(issue({ labels: ['Mid'] }));
  assert.equal(line, 'Fix the footer [MIQR-12] (mid)');
  assert.equal(tagMinutes(line), 25);
  assert.equal(keyOf(line), 'MIQR-12');
  assert.equal(issueLine(issue({ estimate: 2700 })), 'Fix the footer [MIQR-12] (45m)');
});

test('sprint and backlog land in the right sections', () => {
  assert.equal(sectionOf(issue({})), 'TODO');
  assert.equal(sectionOf(issue({ in_sprint: false })), 'PLANNED');
  assert.equal(sectionOf(issue({ category: 'indeterminate' })), 'WIP');
  assert.equal(sectionOf(issue({ category: 'done' })), 'DONE');
  assert.equal(CATEGORY.WIP, 'indeterminate');
});

test('Jira projects read back as normal lists, odd characters included', () => {
  const list = [issue({}), issue({ key: 'MIQR-13', summary: 'Plan: "QR" #1', category: 'indeterminate' }), issue({ key: 'TECH-1', project_key: 'TECH', project_name: 'Technology', in_sprint: false })];
  assert.deepEqual(jiraProjects(list), ['MIQR · miQR', 'TECH · Technology']);
  const p = parseProject('MIQR · miQR', jiraYaml('MIQR · miQR', list));
  assert.equal(p.ok, true);
  assert.deepEqual(p.items.WIP, ['Plan: "QR" #1 [MIQR-13]']);
  assert.deepEqual(p.items.TODO, ['Fix the footer [MIQR-12]']);
  const t = parseProject('TECH · Technology', jiraYaml('TECH · Technology', list));
  assert.deepEqual(t.items.PLANNED, ['Fix the footer [TECH-1]']);
});

test('the task under the cursor is found in PLANNED too', () => {
  const y = 'X:\n  PLANNED:\n  - Later (easy)\n';
  assert.deepEqual(taskAtLine(y, 2), { task: 'Later (easy)', section: 'PLANNED' });
});

test('time spent counts the run, not the pauses', () => {
  let s = start('T', 'P', 25, 0);
  s = toggle(s, 60_000);            // pause after 1 min
  assert.equal(elapsedMs(s, 999_999), 60_000);
  s = toggle(s, 500_000);           // resume
  assert.equal(elapsedMs(s, 560_000), 120_000);
  assert.equal(runEnded(s, toggle(s, 600_000)), false);   // pausing doesn't end the run
  assert.equal(runEnded(s, start('T', 'P', 25, 700_000)), true); // "Need more time? Yes" is a new run
  assert.equal(runEnded(s, idle()), true);
});
