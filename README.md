# Maze Cat

A small focus helper: your task list and a timer that floats over the taskbar.

Pick a task, press Start, and Maze Cat keeps it in front of you with a countdown until it's done. When time is up, the cat wakes up and a soft chime plays. That's the whole idea.

It began as a personal tool that made a real difference to its author, and it is built for people with ADHD, autistic people, and anyone else who says they need it. No proof, no diagnosis asked.

## What it does

- **The bar** sits just above the taskbar and stays on top: the cat, the current task, and the time left. Click the time to pause or resume; drag the task text to move the bar; click the cat to show or hide your lists.
- **Your lists** are plain YAML files, one project per file, so you can also edit them in any text editor or keep them in a synced folder. Or read them from Jira (see Jira Sync below).
- **Tags set each task's timer**: a time like `(15m)`, `(40 min)` or `(1h30)`, or a difficulty: `(easy)` 10 min, `(mid)` 25 min, `(hard)` 1 hour. Tasks without a tag use your default.
- **Click the time** on the bar to change the default (1 h, 25, 5 or 1 min) or to pause.
- **Double-click the bar** to pick a task; double-click a task to start it. When time's up, *Need more time?* Yes runs it again; No marks it done and starts the next task.
- **✓** in the picker marks a task done. Starting a task moves it to `WIP`.

## Your lists

They live in `%LOCALAPPDATA%\MazeCat\Notes` on Windows. Each file holds one project, named like the file:

```yaml
GOVW:
  WIP:
  - Call Jennifer at 11 (mid)
  TODO:
  - Plan one help desk per project (mid)
  - Fix the DMCA notice (easy)
  DONE:
  - Install Maze Cat (easy)
```

An optional `PLANNED` section holds tasks for later; the picker shows only WIP and TODO.

If a file can't be read (two projects in one file, a stray bracket), Maze Cat tells you what's wrong in plain words instead of crashing.

## Jira Sync (read-only)

Switch **Local lists** to **Jira Sync** in the lists window to work from your Jira issues instead of files.

- **Read-only by design.** Maze Cat never creates, edits or deletes issues. It only changes their state: starting a task moves it to *In Progress*, ✓ or *Mark done* moves it to *Done*. To add or edit tasks, use Jira, or switch back to Local lists.
- **Time spent is logged.** When a task moves to Done, the time the timer ran on it (pauses don't count) is added to the issue as a work log, in whole minutes.
- **Sprint first.** Issues in an open sprint show under TODO / WIP / DONE; the top 25 backlog items show under PLANNED.
- **Tags still work.** A label `easy`, `mid` or `hard` sets the timer; otherwise the original estimate does.
- **Your token stays private.** It goes straight to Windows Credential Manager. Make one at id.atlassian.com → Security → API tokens.
- **Filter:** by default, your open issues plus those done in the last 7 days. Change it with any JQL in Jira settings.

Planned: a separate backlog popup, and the same read-only mode for other boards (Trello, Asana and more).

## Install (Windows)

Until there's a signed release, take the installer from the latest successful run on the **Actions** tab (artifact **mazecat-windows**). Windows SmartScreen will warn that the app isn't recognized: choose **More info**, then **Run anyway**. Code signing is planned.

## Build it yourself

You need [Node.js 22](https://nodejs.org/), [Rust](https://rustup.rs/) and, on Windows, the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```powershell
npm ci
npm test              # list and timer logic
npx tauri dev         # run with hot reload
npx tauri build       # make an installer
```

## How it's built

[Tauri 2](https://v2.tauri.app/): a small Rust core that reads and writes your list files, and two web windows (the bar and your lists) written in TypeScript with no framework. The list logic in `src/lists.ts` and the timer in `src/timer.ts` have no Tauri code, so they're easy to test and change.

## Principles

- **Nothing about us without us.** Neurodivergent people help design and test it.
- **Calm by default.** Quiet colors, no flashing, no surprise sounds beyond the chime you asked for.
- **Your data stays yours.** Plain files on your computer; no account, no tracking.

## Contributing

Ideas, bug reports and pull requests are welcome. Open an issue first for anything big. Please keep changes small, keep the bar calm, and run `npm test` before you push.

## License

[MIT](LICENSE) © 2026 Brian Thorne
