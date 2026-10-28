import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseProject, taskAtLine, tagMinutes, moveTask, firstTodo } from '../src/lists.ts';
import { start, toggle, remaining, format } from '../src/timer.ts';

const sample = 'GOVW:\n  WIP:\n  - Call Jennifer 11:00 (mid)\n  TODO:\n  - Fix "512©(3)" – find the swap (easy)\n  - Plan: help desks per project (mid)\n  DONE:\n  - Install Maze Cat (easy)\n';

test('reads one project per file', () => {
  const p = parseProject('GOVW', sample);
  assert.equal(p.ok, true);
  assert.deepEqual(p.items.WIP, ['Call Jennifer 11:00 (mid)']);
  assert.equal(p.items.TODO[1], 'Plan: help desks per project (mid)'); // "a: b" stays plain text
});

test('explains a file with several projects instead of crashing', () => {
  const p = parseProject('20261028', 'A:\n  TODO:\n  - x\nB:\n  TODO:\n  - y\n');
  assert.equal(p.ok, false);
  assert.match(p.message, /2 projects/);
});

test('a quoted word before a colon stays a plain task', () => {
  const p = parseProject('X', 'X:\n  TODO:\n  - PoC "flicker text": one SVG\n');
  assert.equal(p.ok, true);
  assert.equal(p.items.TODO[0], 'PoC "flicker text": one SVG');
});

test('explains broken YAML', () => {
  const p = parseProject('X', 'X:\n  TODO:\n  - [unclosed\n');
  assert.equal(p.ok, false);
  assert.match(p.message, /isn't valid YAML/);
});

test('difficulty tags set the time', () => {
  assert.equal(tagMinutes('Ship it (hard)'), 60);
  assert.equal(tagMinutes('Ship it (Easy) '), 10);
  assert.equal(tagMinutes('Ship it'), null);
  assert.equal(tagMinutes('Review the brief (15m)'), 15);
  assert.equal(tagMinutes('Review the brief (mid) (40 min)'), 40);
  assert.equal(tagMinutes('Deep work (hard, 1h30)'), 90);
  assert.equal(tagMinutes('Deep work (1.5h)'), 90);
  assert.equal(tagMinutes('Call (Ana) about rent'), null);
});

test('finds the task and section under the cursor', () => {
  assert.deepEqual(taskAtLine(sample, 4), { task: 'Fix "512©(3)" – find the swap (easy)', section: 'TODO' });
  assert.equal(taskAtLine(sample, 3), null);
});

test('moving a task keeps every other line as written', () => {
  const out = moveTask(sample, 'Plan: help desks per project (mid)', 'DONE');
  assert.equal(out.split('\n').length, sample.split('\n').length);
  const p = parseProject('GOVW', out);
  assert.deepEqual(p.items.DONE, ['Plan: help desks per project (mid)', 'Install Maze Cat (easy)']);
  assert.equal(p.items.TODO.length, 1);
});

test('timer pauses, resumes and restarts when time is up', () => {
  let s = start('T', 'P', 1, 0);
  assert.equal(remaining(s, 20_000), 40_000);
  s = toggle(s, 20_000);                 // pause
  assert.equal(remaining(s, 99_000), 40_000);
  s = toggle(s, 100_000);                // resume
  assert.equal(remaining(s, 110_000), 30_000);
  assert.equal(remaining(s, 999_000), 0);
  assert.equal(format(25 * 60_000), '25:00');
  assert.equal(format(3_600_000), '1:00:00');
});

test('"No, it\'s done" picks the top task under TODO', () => {
  let t = moveTask(sample, 'Call Jennifer 11:00 (mid)', 'DONE');
  const next = firstTodo('GOVW', t);
  assert.equal(next, 'Fix "512©(3)" – find the swap (easy)');
  t = moveTask(t, next, 'WIP');
  assert.deepEqual(parseProject('GOVW', t).items.WIP, [next]);
});
