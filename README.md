# Maze Cat

A small focus helper: your task list and a timer that floats over the taskbar.

Pick a task, press Start, and Maze Cat keeps it in front of you with a countdown until it's done. When time is up, the cat wakes up and a soft chime plays. That's the whole idea.

It began as a personal tool that made a real difference to its author, and it is built for people with ADHD, autistic people, and anyone else who says they need it. No proof, no diagnosis asked.

## What it does

- **The bar** sits just above the taskbar and stays on top: the cat, the current task, and the time left. Click the time to pause or resume; drag the task text to move the bar; click the cat to show or hide your lists.
- **Your lists** are plain YAML files, one project per file, so you can also edit them in any text editor or keep them in a synced folder.
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

If a file can't be read (two projects in one file, a stray bracket), Maze Cat tells you what's wrong in plain words instead of crashing.

## Install (Windows)

Until there's a signed release, take the installer from the latest successful run on the **Actions** tab (artifact **mazecat-windows**). Windows SmartScreen will warn that the app isn't recognized: choose **More info**, then **Run anyway**. Code signing is planned.

### Keep the bar above the taskbar (local build)

Windows keeps the taskbar above ordinary "always on top" windows. A window can sit above it only if its program's manifest asks for `uiAccess`, the program is signed by a certificate the computer trusts, and it's installed under Program Files. The web engine Maze Cat's windows use stops drawing under `uiAccess`, so this version adds a second, small native program for the bar (`bar/`, `mazecat-bar.exe`). It draws the cat, the task and the timer, and the app keeps all the logic. They talk over a local connection protected by a one-time token. The bar shows in Alt+Tab like any open app; right-click it to pick a task, show your lists or quit.

Until there's a Microsoft Store release, you can build that version for your own computer:

1. Once, in an **admin** PowerShell: `powershell -ExecutionPolicy Bypass -File scripts\make-local-cert.ps1`. This makes a signing certificate that only this computer trusts.
2. `powershell -ExecutionPolicy Bypass -File scripts\build-local.ps1`. This builds and signs both programs and the installer.
3. Quit Maze Cat if it's running, then run the new installer. It asks for admin because it installs for all users under Program Files.
4. Open Maze Cat from the Start menu (as a normal user, not "Run as administrator").

Your lists stay where they are. `npx tauri dev` and the Actions builds don't include the native bar, so they show the regular web bar.

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
