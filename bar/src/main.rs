//! Maze Cat's bar: the cat, the current task and the timer, sitting on the taskbar.
//!
//! A small native program (no web engine), so it can carry the "above the taskbar"
//! permission (uiAccess). The main Maze Cat app starts it and keeps all the logic:
//! lists, picker, notes. The two talk over a local connection, one JSON message per line.
//!
//! main -> bar: {"t":"state", task, project, totalMs, endsAt, leftMs, def}
//!              {"t":"anchor", right, y}   (physical pixels: right edge and top)
//! bar -> main: hello(token), cat, pick, lists, preset(min), pause, moved(right, y), up, quit
#![cfg_attr(windows, windows_subsystem = "windows")]

use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi as gdi;
use windows_sys::Win32::Graphics::GdiPlus as gp;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::Shell::ExtractIconExW;
use windows_sys::Win32::UI::WindowsAndMessaging as wm;

const PIXEL_FORMAT_32BPP_PARGB: i32 = 0x000E_200B;
const WM_NET: u32 = wm::WM_APP + 1;
const TIMER_TICK: usize = 1;
const TIMER_TOP: usize = 2;
const HEIGHT: f32 = 40.0;
const WM_MOUSELEAVE: u32 = 0x02A3;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn now_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

// ---------- the link to the main app ----------

static OUT: Mutex<Option<TcpStream>> = Mutex::new(None);
static HWND_BAR: AtomicIsize = AtomicIsize::new(0);

// ---------- "above the taskbar": check it, and ask for the right z-order band ----------

/// True when Windows actually granted uiAccess (signed, under Program Files, started through the shell).
fn has_ui_access() -> bool {
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUIAccess, TOKEN_QUERY};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut on: u32 = 0;
        let mut len = 0u32;
        let ok = GetTokenInformation(token, TokenUIAccess, &mut on as *mut u32 as *mut _, 4, &mut len);
        windows_sys::Win32::Foundation::CloseHandle(token);
        ok != 0 && on != 0
    }
}

/// Windows keeps windows in z-order "bands"; the taskbar sits in the desktop band and
/// uiAccess windows can live in the band above it (ZBID_UIACCESS = 2). These two calls
/// are undocumented user32 exports (used by accessibility tools), so look them up and
/// fall back gracefully when they're missing.
const ZBID_UIACCESS: u32 = 2;
type CreateWindowInBandFn = unsafe extern "system" fn(u32, PCWSTR, PCWSTR, u32, i32, i32, i32, i32, HWND, wm::HMENU, windows_sys::Win32::Foundation::HINSTANCE, *const core::ffi::c_void, u32) -> HWND;
type GetWindowBandFn = unsafe extern "system" fn(HWND, *mut u32) -> i32;

fn user32(name: &str) -> windows_sys::Win32::Foundation::FARPROC {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    let lib = wide("user32.dll");
    let mut n = name.as_bytes().to_vec();
    n.push(0);
    unsafe { GetProcAddress(LoadLibraryW(lib.as_ptr()), n.as_ptr()) }
}

fn window_band(h: HWND) -> Option<u32> {
    let f = user32("GetWindowBand")?;
    let f: GetWindowBandFn = unsafe { std::mem::transmute(f) };
    let mut band = 0u32;
    (unsafe { f(h, &mut band) } != 0).then_some(band)
}

/// One line about how the bar is running, for the right-click menu and the log.
fn status(h: HWND) -> String {
    let ui = if has_ui_access() { "on" } else { "off" };
    match window_band(h) {
        Some(b) => format!("Above-taskbar permission: {ui} (band {b})"),
        None => format!("Above-taskbar permission: {ui}"),
    }
}

/// Writes the status next to your lists (%LOCALAPPDATA%\MazeCat\bar.log), to help when the bar misbehaves.
fn log(line: &str) {
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        let dir = std::path::Path::new(&dir).join("MazeCat");
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("bar.log")) {
            let _ = writeln!(f, "{:.0} {line}", now_ms() / 1000.0);
        }
    }
}

fn send(v: Value) {
    if let Ok(mut out) = OUT.lock() {
        if let Some(s) = out.as_mut() {
            let _ = writeln!(s, "{v}");
        }
    }
}

fn arg(name: &str) -> Option<String> {
    let mut it = std::env::args();
    while let Some(a) = it.next() {
        if a == name {
            return it.next();
        }
    }
    None
}

// ---------- the timer, same rules as src/timer.ts ----------

#[derive(Default)]
struct Timer {
    task: String,
    total_ms: f64,
    ends_at: Option<f64>,
    left_ms: f64,
}

impl Timer {
    fn remaining(&self) -> f64 {
        match self.ends_at {
            Some(e) => (e - now_ms()).max(0.0),
            None => self.left_ms,
        }
    }
    fn up(&self) -> bool {
        !self.task.is_empty() && self.total_ms > 0.0 && self.remaining() <= 0.0
    }
}

fn format(ms: f64) -> String {
    let total = (ms / 1000.0).ceil() as i64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

// ---------- colors: follow the taskbar's light or dark theme ----------

struct Theme {
    fg: u32,
    muted: u32,
    accent: u32,
    panel: u32,
    line: u32,
    hover: u32,
    sep: u32,
    cat_face: u32,
    on_accent: u32,
}

fn taskbar_is_light() -> bool {
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let val = wide("SystemUsesLightTheme");
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), &mut data as *mut u32 as *mut _, &mut size)
    };
    r == 0 && data == 1
}

fn theme(light: bool) -> Theme {
    if light {
        Theme { fg: 0xFF1B1B1B, muted: 0xFF5C6370, accent: 0xFFA86500, panel: 0x18000000, line: 0x33000000, hover: 0x10000000, sep: 0x2E000000, cat_face: 0xFFF3F3F3, on_accent: 0xFFFFFFFF }
    } else {
        Theme { fg: 0xFFE9E6DF, muted: 0xFF9AA3AF, accent: 0xFFF2B84B, panel: 0x22FFFFFF, line: 0x40FFFFFF, hover: 0x10FFFFFF, sep: 0x2EFFFFFF, cat_face: 0xFF15181E, on_accent: 0xFF15181E }
    }
}

// ---------- state of the window ----------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    None,
    Task,
    Cat,
    Clock,
    Preset(u32),
    Pause,
    Close,
}

const PRESETS: [(u32, &str); 4] = [(60, "1 h"), (25, "25 min"), (5, "5 min"), (1, "1 min")];

struct Fonts {
    task: *mut gp::GpFont,
    clock: *mut gp::GpFont,
    fmt_task: *mut gp::GpStringFormat,
    fmt_center: *mut gp::GpStringFormat,
    measure: *mut gp::GpGraphics,
}

struct Bar {
    hwnd: HWND,
    timer: Timer,
    def_min: u32,
    anchor: Option<(i32, i32)>,
    presets: bool,
    hover: Hit,
    press: Option<(Hit, i32, i32)>,
    tracking: bool,
    moving: bool,
    rang: bool,
    shown: bool,
    got_state: bool,
    light: bool,
    key: String,
    hits: Vec<(Hit, gp::RectF)>,
    fonts: Fonts,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
    /// Messages that arrived while the bar was busy (a menu open): handled on the next tick.
    static PENDING: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
}

fn with_bar<R>(f: impl FnOnce(&mut Bar) -> R) -> Option<R> {
    BAR.with(|b| b.try_borrow_mut().ok().and_then(|mut b| b.as_mut().map(f)))
}

// ---------- drawing helpers (GDI+, anti-aliased, with real transparency) ----------

unsafe fn brush(color: u32) -> *mut gp::GpSolidFill {
    let mut b = std::ptr::null_mut();
    gp::GdipCreateSolidFill(color, &mut b);
    b
}

unsafe fn fill_round(g: *mut gp::GpGraphics, color: u32, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let mut p = std::ptr::null_mut();
    gp::GdipCreatePath(gp::FillModeAlternate, &mut p);
    let d = r * 2.0;
    gp::GdipAddPathArc(p, x, y, d, d, 180.0, 90.0);
    gp::GdipAddPathArc(p, x + w - d, y, d, d, 270.0, 90.0);
    gp::GdipAddPathArc(p, x + w - d, y + h - d, d, d, 0.0, 90.0);
    gp::GdipAddPathArc(p, x, y + h - d, d, d, 90.0, 90.0);
    gp::GdipClosePathFigure(p);
    let b = brush(color);
    gp::GdipFillPath(g, b as *mut gp::GpBrush, p);
    gp::GdipDeleteBrush(b as *mut gp::GpBrush);
    gp::GdipDeletePath(p);
}

unsafe fn stroke_round(g: *mut gp::GpGraphics, color: u32, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let mut p = std::ptr::null_mut();
    gp::GdipCreatePath(gp::FillModeAlternate, &mut p);
    let d = r * 2.0;
    gp::GdipAddPathArc(p, x, y, d, d, 180.0, 90.0);
    gp::GdipAddPathArc(p, x + w - d, y, d, d, 270.0, 90.0);
    gp::GdipAddPathArc(p, x + w - d, y + h - d, d, d, 0.0, 90.0);
    gp::GdipAddPathArc(p, x, y + h - d, d, d, 90.0, 90.0);
    gp::GdipClosePathFigure(p);
    let mut pen = std::ptr::null_mut();
    gp::GdipCreatePen1(color, 1.0, gp::UnitPixel, &mut pen);
    gp::GdipDrawPath(g, pen, p);
    gp::GdipDeletePen(pen);
    gp::GdipDeletePath(p);
}

unsafe fn text(g: *mut gp::GpGraphics, s: &str, font: *mut gp::GpFont, fmt: *mut gp::GpStringFormat, color: u32, r: gp::RectF) {
    let w: Vec<u16> = s.encode_utf16().collect();
    let b = brush(color);
    gp::GdipDrawString(g, w.as_ptr(), w.len() as i32, font, &r, fmt, b as *const gp::GpBrush);
    gp::GdipDeleteBrush(b as *mut gp::GpBrush);
}

unsafe fn measure(f: &Fonts, s: &str, font: *mut gp::GpFont, fmt: *mut gp::GpStringFormat, max_w: f32, max_h: f32) -> gp::RectF {
    let w: Vec<u16> = s.encode_utf16().collect();
    let layout = gp::RectF { X: 0.0, Y: 0.0, Width: max_w, Height: max_h };
    let mut out = gp::RectF::default();
    gp::GdipMeasureString(f.measure, w.as_ptr(), w.len() as i32, font, &layout, fmt, &mut out, std::ptr::null_mut(), std::ptr::null_mut());
    out
}

/// Maze Cat's mark, the same shapes as src/cat.ts (a 24-unit square), drawn in a 26 px button.
unsafe fn draw_cat(g: *mut gp::GpGraphics, x: f32, y: f32, awake: bool, t: &Theme) {
    let mut state = 0u32;
    gp::GdipSaveGraphics(g, &mut state);
    gp::GdipTranslateWorldTransform(g, x + 2.0, y + 2.0, gp::MatrixOrderPrepend);
    gp::GdipScaleWorldTransform(g, 22.0 / 24.0, 22.0 / 24.0, gp::MatrixOrderPrepend);

    let mut head = std::ptr::null_mut();
    gp::GdipCreatePath(gp::FillModeAlternate, &mut head);
    let pts = [(4.0, 20.0), (4.0, 7.5), (7.6, 3.0), (10.6, 7.2), (13.4, 7.2), (16.4, 3.0), (20.0, 7.5), (20.0, 20.0)];
    for w in pts.windows(2) {
        gp::GdipAddPathLine(head, w[0].0, w[0].1, w[1].0, w[1].1);
    }
    gp::GdipAddPathArc(head, 16.0, 18.0, 4.0, 4.0, 0.0, 90.0);
    gp::GdipAddPathLine(head, 18.0, 22.0, 6.0, 22.0);
    gp::GdipAddPathArc(head, 4.0, 18.0, 4.0, 4.0, 90.0, 90.0);
    gp::GdipClosePathFigure(head);
    let body = brush(if awake { t.accent } else { t.fg });
    gp::GdipFillPath(g, body as *mut gp::GpBrush, head);
    gp::GdipDeleteBrush(body as *mut gp::GpBrush);
    gp::GdipDeletePath(head);

    let face = brush(t.cat_face);
    if awake {
        gp::GdipFillEllipse(g, face as *mut gp::GpBrush, 9.2 - 1.6, 13.0 - 1.6, 3.2, 3.2);
        gp::GdipFillEllipse(g, face as *mut gp::GpBrush, 14.8 - 1.6, 13.0 - 1.6, 3.2, 3.2);
    } else {
        // Closed eyes: the two small curves of the SVG (quadratics, here as cubics).
        let mut pen = std::ptr::null_mut();
        gp::GdipCreatePen1(t.cat_face, 1.3, gp::UnitPixel, &mut pen);
        gp::GdipSetPenLineCap197819(pen, gp::LineCapRound, gp::LineCapRound, gp::DashCapRound);
        for dx in [0.0f32, 5.6] {
            let mut eye = std::ptr::null_mut();
            gp::GdipCreatePath(gp::FillModeAlternate, &mut eye);
            gp::GdipAddPathBezier(eye, 7.8 + dx, 13.2, 8.7333 + dx, 12.4, 9.6667 + dx, 12.4, 10.6 + dx, 13.2);
            gp::GdipDrawPath(g, pen, eye);
            gp::GdipDeletePath(eye);
        }
        gp::GdipDeletePen(pen);
    }
    let nose = [gp::PointF { X: 11.0, Y: 16.4 }, gp::PointF { X: 13.0, Y: 16.4 }, gp::PointF { X: 12.0, Y: 17.5 }];
    gp::GdipFillPolygon(g, face as *mut gp::GpBrush, nose.as_ptr(), 3, gp::FillModeAlternate);
    gp::GdipDeleteBrush(face as *mut gp::GpBrush);
    gp::GdipRestoreGraphics(g, state);
}

// ---------- layout and paint ----------

struct Item {
    hit: Hit,
    r: gp::RectF,
    label: String,
}

fn layout(b: &Bar) -> (f32, Vec<Item>) {
    let f = &b.fonts;
    let mut items = Vec::new();
    unsafe {
        if b.presets {
            let mut x = 4.0;
            let pause = if b.timer.ends_at.is_none() && !b.timer.task.is_empty() { "Resume" } else { "Pause" };
            let mut labels: Vec<(Hit, String)> = PRESETS.iter().map(|(m, l)| (Hit::Preset(*m), l.to_string())).collect();
            labels.push((Hit::Pause, pause.into()));
            labels.push((Hit::Close, "\u{2715}".into()));
            for (hit, label) in labels {
                let tw = measure(f, &label, f.task, f.fmt_center, 400.0, 40.0).Width;
                let w = if hit == Hit::Close { 28.0 } else { (tw + 16.0).max(44.0).ceil() };
                items.push(Item { hit, r: gp::RectF { X: x, Y: 6.0, Width: w, Height: 28.0 }, label });
                x += w + 4.0;
            }
            (x, items)
        } else {
            let label = task_label(&b.timer);
            let tw = measure(f, &label, f.task, f.fmt_task, 210.0, 31.0).Width.ceil().clamp(20.0, 210.0);
            let mut x = 17.0;
            items.push(Item { hit: Hit::Task, r: gp::RectF { X: x, Y: 4.5, Width: tw, Height: 31.0 }, label });
            x += tw + 6.0;
            items.push(Item { hit: Hit::Cat, r: gp::RectF { X: x, Y: 7.0, Width: 26.0, Height: 26.0 }, label: String::new() });
            x += 26.0 + 6.0;
            let clock = clock_label(&b.timer);
            let len = measure(f, &clock, f.clock, f.fmt_center, 200.0, 40.0).Width.ceil();
            let h = (len + 4.0).min(38.0);
            items.push(Item { hit: Hit::Clock, r: gp::RectF { X: x, Y: (HEIGHT - h) / 2.0, Width: 18.0, Height: h }, label: clock });
            x += 18.0 + 4.0;
            (x, items)
        }
    }
}

fn task_label(t: &Timer) -> String {
    if t.task.is_empty() {
        "Pick a task".into()
    } else if t.up() {
        format!("Time's up \u{00b7} {}", t.task)
    } else {
        t.task.clone()
    }
}

fn clock_label(t: &Timer) -> String {
    if t.task.is_empty() { "--:--".into() } else { format(t.remaining()) }
}

fn paint(b: &mut Bar) {
    let Some((right, top)) = b.anchor else { return };
    if !b.got_state {
        return;
    }
    let scale = unsafe { GetDpiForWindow(b.hwnd) } as f32 / 96.0;
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (w_logical, items) = layout(b);
    let w = (w_logical * scale).ceil() as i32;
    let h = (HEIGHT * scale).ceil() as i32;
    let t = theme(b.light);
    let up = b.timer.up();

    unsafe {
        let screen = gdi::GetDC(std::ptr::null_mut());
        let mem = gdi::CreateCompatibleDC(screen);
        let mut bmi: gdi::BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<gdi::BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = gdi::BI_RGB;
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = gdi::CreateDIBSection(mem, &bmi, gdi::DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            gdi::DeleteDC(mem);
            gdi::ReleaseDC(std::ptr::null_mut(), screen);
            return;
        }
        let old = gdi::SelectObject(mem, dib as gdi::HGDIOBJ);

        let mut bmp = std::ptr::null_mut();
        gp::GdipCreateBitmapFromScan0(w, h, w * 4, PIXEL_FORMAT_32BPP_PARGB, bits as *const u8, &mut bmp);
        let mut g = std::ptr::null_mut();
        gp::GdipGetImageGraphicsContext(bmp as *mut gp::GpImage, &mut g);
        gp::GdipSetSmoothingMode(g, gp::SmoothingModeAntiAlias);
        gp::GdipSetTextRenderingHint(g, gp::TextRenderingHintAntiAliasGridFit);
        // Almost invisible, but not zero: fully clear pixels would let clicks fall through to the taskbar.
        gp::GdipGraphicsClear(g, 0x01000000);
        gp::GdipScaleWorldTransform(g, scale, scale, gp::MatrixOrderPrepend);

        b.hits.clear();
        if b.presets {
            for it in &items {
                let r = it.r;
                let pressed = matches!(it.hit, Hit::Preset(m) if m == b.def_min);
                let disabled = it.hit == Hit::Pause && b.timer.task.is_empty();
                fill_round(g, if b.hover == it.hit && !disabled { t.line } else { t.panel }, r.X, r.Y, r.Width, r.Height, 7.0);
                stroke_round(g, if pressed { t.accent } else { t.line }, r.X + 0.5, r.Y + 0.5, r.Width - 1.0, r.Height - 1.0, 7.0);
                let color = if pressed { t.accent } else if disabled || it.hit == Hit::Close { t.muted } else { t.fg };
                text(g, &it.label, b.fonts.task, b.fonts.fmt_center, color, r);
                if !disabled {
                    b.hits.push((it.hit, r));
                }
            }
        } else {
            if b.hover != Hit::None {
                fill_round(g, t.hover, 0.0, 0.0, w_logical, HEIGHT, 8.0);
            }
            let sep = brush(t.sep);
            gp::GdipFillRectangle(g, sep as *mut gp::GpBrush, 8.0, 8.0, 1.0, HEIGHT - 16.0);
            gp::GdipDeleteBrush(sep as *mut gp::GpBrush);
            for it in &items {
                let r = it.r;
                match it.hit {
                    Hit::Task => {
                        let color = if b.timer.task.is_empty() { t.muted } else { t.fg };
                        text(g, &it.label, b.fonts.task, b.fonts.fmt_task, color, r);
                        // The whole left part (separator to cat) picks and drags.
                        b.hits.push((Hit::Task, gp::RectF { X: 0.0, Y: 0.0, Width: r.X + r.Width + 3.0, Height: HEIGHT }));
                    }
                    Hit::Cat => {
                        if b.hover == Hit::Cat {
                            fill_round(g, t.panel, r.X, r.Y, r.Width, r.Height, 7.0);
                        }
                        draw_cat(g, r.X, r.Y, up, &t);
                        b.hits.push((Hit::Cat, r));
                    }
                    Hit::Clock => {
                        let color = if up {
                            fill_round(g, t.accent, r.X, r.Y, r.Width, r.Height, 6.0);
                            t.on_accent
                        } else {
                            if b.hover == Hit::Clock {
                                fill_round(g, t.panel, r.X, r.Y, r.Width, r.Height, 6.0);
                            }
                            if b.timer.ends_at.is_some() && !b.timer.task.is_empty() { t.accent } else { t.muted }
                        };
                        // Vertical, read bottom to top, like the first Maze Cat.
                        let mut st = 0u32;
                        gp::GdipSaveGraphics(g, &mut st);
                        gp::GdipTranslateWorldTransform(g, r.X + r.Width / 2.0, r.Y + r.Height / 2.0, gp::MatrixOrderPrepend);
                        gp::GdipRotateWorldTransform(g, -90.0, gp::MatrixOrderPrepend);
                        // Long times (1:00:00) shrink to fit the bar's height instead of losing digits.
                        let len = measure(&b.fonts, &it.label, b.fonts.clock, b.fonts.fmt_center, 400.0, 40.0).Width;
                        let fit = ((r.Height - 2.0) / len).min(1.0);
                        if fit < 1.0 {
                            gp::GdipScaleWorldTransform(g, fit, fit, gp::MatrixOrderPrepend);
                        }
                        let rr = gp::RectF { X: -len / 2.0 - 4.0, Y: -r.Width / 2.0, Width: len + 8.0, Height: r.Width };
                        text(g, &it.label, b.fonts.clock, b.fonts.fmt_center, color, rr);
                        gp::GdipRestoreGraphics(g, st);
                        b.hits.push((Hit::Clock, r));
                    }
                    _ => {}
                }
            }
        }
        gp::GdipDeleteGraphics(g);
        gp::GdipDisposeImage(bmp as *mut gp::GpImage);

        let size = SIZE { cx: w, cy: h };
        let src = POINT { x: 0, y: 0 };
        let dst = POINT { x: right - w, y: top };
        let blend = gdi::BLENDFUNCTION { BlendOp: gdi::AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: gdi::AC_SRC_ALPHA as u8 };
        // While you drag it, Windows owns the position: only redraw in place.
        let pdst = if b.moving { std::ptr::null() } else { &dst as *const POINT };
        wm::UpdateLayeredWindow(b.hwnd, screen, pdst, &size, mem, &src, 0, &blend, wm::ULW_ALPHA);

        gdi::SelectObject(mem, old);
        gdi::DeleteObject(dib as gdi::HGDIOBJ);
        gdi::DeleteDC(mem);
        gdi::ReleaseDC(std::ptr::null_mut(), screen);

        if !b.shown {
            b.shown = true;
            wm::ShowWindow(b.hwnd, wm::SW_SHOWNOACTIVATE);
            raise(b.hwnd);
        }
    }
}

fn raise(h: HWND) {
    unsafe {
        wm::SetWindowPos(h, wm::HWND_TOPMOST, 0, 0, 0, 0, wm::SWP_NOMOVE | wm::SWP_NOSIZE | wm::SWP_NOACTIVATE);
    }
}

// ---------- the chime: two soft notes, made on the fly (no sound files to license) ----------

fn chime() {
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(|| {
        let rate = 22050u32;
        let n = (rate as f32 * 1.3) as usize;
        let mut samples = vec![0f32; n];
        for (i, f) in [660f32, 880.0].iter().enumerate() {
            let t0 = i as f32 * 0.28;
            for (k, s) in samples.iter_mut().enumerate() {
                let t = k as f32 / rate as f32 - t0;
                if !(0.0..1.0).contains(&t) {
                    continue;
                }
                // Same envelope as the web bar: quick rise to 0.18, slow fade by 0.9 s.
                let env = if t < 0.03 { 0.0001 * (0.18f32 / 0.0001).powf(t / 0.03) } else { 0.18 * (0.0001f32 / 0.18).powf(((t - 0.03) / 0.87).min(1.0)) };
                *s += env * (std::f32::consts::TAU * f * t).sin();
            }
        }
        let data: Vec<u8> = samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut w = Vec::with_capacity(44 + data.len());
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes()); // PCM
        w.extend_from_slice(&1u16.to_le_bytes()); // mono
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(&data);
        w
    });
    unsafe {
        PlaySoundW(wav.as_ptr() as PCWSTR, std::ptr::null_mut(), SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

// ---------- events ----------

fn hit_at(b: &Bar, lparam: LPARAM) -> Hit {
    let x = (lparam & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as f32;
    let scale = (unsafe { GetDpiForWindow(b.hwnd) } as f32 / 96.0).max(0.5);
    let (x, y) = (x / scale, y / scale);
    b.hits
        .iter()
        .find(|(_, r)| x >= r.X && x < r.X + r.Width && y >= r.Y && y < r.Y + r.Height)
        .map(|(h, _)| *h)
        .unwrap_or(Hit::None)
}

fn click(b: &mut Bar, hit: Hit) {
    match hit {
        Hit::Cat => send(json!({"t": "cat"})),
        Hit::Clock => b.presets = true,
        Hit::Preset(m) => {
            b.def_min = m;
            b.presets = false;
            send(json!({"t": "preset", "min": m}));
        }
        Hit::Pause => {
            b.presets = false;
            send(json!({"t": "pause"}));
        }
        Hit::Close => b.presets = false,
        _ => {}
    }
    b.hover = Hit::None;
    paint(b);
}

fn menu(hwnd: HWND) {
    unsafe {
        let m = wm::CreatePopupMenu();
        let info = status(hwnd);
        let w = wide(&info);
        wm::AppendMenuW(m, wm::MF_STRING | wm::MF_GRAYED, 99, w.as_ptr());
        wm::AppendMenuW(m, wm::MF_SEPARATOR, 0, std::ptr::null());
        let items = [(1usize, "Pick a task"), (2, "Show or hide your lists"), (0, ""), (3, "Quit Maze Cat")];
        for (id, label) in items {
            if id == 0 {
                wm::AppendMenuW(m, wm::MF_SEPARATOR, 0, std::ptr::null());
            } else {
                let w = wide(label);
                wm::AppendMenuW(m, wm::MF_STRING, id, w.as_ptr());
            }
        }
        let mut pt = POINT { x: 0, y: 0 };
        wm::GetCursorPos(&mut pt);
        // A menu needs its window in front, or it never closes when you click elsewhere.
        wm::SetForegroundWindow(hwnd);
        let cmd = wm::TrackPopupMenu(m, wm::TPM_RETURNCMD | wm::TPM_RIGHTBUTTON | wm::TPM_BOTTOMALIGN | wm::TPM_RIGHTALIGN, pt.x, pt.y, 0, hwnd, std::ptr::null());
        wm::PostMessageW(hwnd, wm::WM_NULL, 0, 0);
        wm::DestroyMenu(m);
        match cmd {
            1 => send(json!({"t": "pick"})),
            2 => send(json!({"t": "lists"})),
            3 => {
                send(json!({"t": "quit"}));
                wm::PostMessageW(hwnd, wm::WM_CLOSE, 0, 0);
            }
            _ => {}
        }
    }
}

fn on_net(b: &mut Bar, v: Value) {
    match v["t"].as_str() {
        Some("state") => {
            b.timer = Timer {
                task: v["task"].as_str().unwrap_or("").to_string(),
                total_ms: v["totalMs"].as_f64().unwrap_or(0.0),
                ends_at: v["endsAt"].as_f64(),
                left_ms: v["leftMs"].as_f64().unwrap_or(0.0),
            };
            if let Some(d) = v["def"].as_u64() {
                b.def_min = d as u32;
            }
            b.got_state = true;
        }
        Some("anchor") => {
            if let (Some(r), Some(y)) = (v["right"].as_i64(), v["y"].as_i64()) {
                b.anchor = Some((r as i32, y as i32));
            }
        }
        _ => {}
    }
    paint(b);
}

fn drain(b: &mut Bar) {
    let msgs: Vec<Value> = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for v in msgs {
        on_net(b, v);
    }
}

fn tick(b: &mut Bar) {
    let up = b.timer.up();
    if up && !b.rang {
        b.rang = true;
        chime();
        send(json!({"t": "up"}));
    }
    if !up {
        b.rang = false;
    }
    let key = format!("{}|{}|{}", clock_label(&b.timer), up, b.presets);
    if key != b.key {
        b.key = key;
        paint(b);
    }
}

unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    let h = HWND_BAR.load(Ordering::Relaxed);
    if h != 0 {
        raise(h as HWND);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = match msg {
        WM_NET => {
            let v = *Box::from_raw(lparam as *mut Value);
            PENDING.with(|p| p.borrow_mut().push(v));
            with_bar(drain);
            Some(0)
        }
        wm::WM_TIMER if wparam == TIMER_TICK => {
            with_bar(|b| {
                drain(b);
                tick(b);
            });
            Some(0)
        }
        wm::WM_TIMER if wparam == TIMER_TOP => {
            raise(hwnd);
            Some(0)
        }
        wm::WM_MOUSEACTIVATE => Some(wm::MA_NOACTIVATE as LRESULT),
        wm::WM_MOUSEMOVE => {
            with_bar(|b| {
                if !b.tracking {
                    let mut tme = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                    TrackMouseEvent(&mut tme);
                    b.tracking = true;
                }
                if let Some((Hit::Task, sx, sy)) = b.press {
                    let x = (lparam & 0xFFFF) as u16 as i16 as i32;
                    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
                    if (x - sx).abs() > 4 || (y - sy).abs() > 4 {
                        b.press = None;
                        ReleaseCapture();
                        // Let Windows move it, like dragging a title bar.
                        wm::PostMessageW(hwnd, wm::WM_NCLBUTTONDOWN, wm::HTCAPTION as WPARAM, 0);
                        return;
                    }
                }
                let h = hit_at(b, lparam);
                let h = if h == Hit::None && !b.presets { Hit::Task } else { h };
                if h != b.hover {
                    b.hover = h;
                    paint(b);
                }
            });
            Some(0)
        }
        WM_MOUSELEAVE => {
            with_bar(|b| {
                b.tracking = false;
                if b.hover != Hit::None {
                    b.hover = Hit::None;
                    paint(b);
                }
            });
            Some(0)
        }
        wm::WM_LBUTTONDOWN => {
            with_bar(|b| {
                let x = (lparam & 0xFFFF) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;
                let h = hit_at(b, lparam);
                let h = if h == Hit::None && !b.presets { Hit::Task } else { h };
                b.press = Some((h, x, y));
                SetCapture(hwnd);
            });
            Some(0)
        }
        wm::WM_LBUTTONUP => {
            ReleaseCapture();
            with_bar(|b| {
                if let Some((h, _, _)) = b.press.take() {
                    if h == hit_at(b, lparam) && h != Hit::Task && h != Hit::None {
                        click(b, h);
                    }
                }
            });
            Some(0)
        }
        wm::WM_LBUTTONDBLCLK => {
            with_bar(|b| {
                let h = hit_at(b, lparam);
                if !b.presets && (h == Hit::Task || h == Hit::None) {
                    send(json!({"t": "pick"}));
                } else if h != Hit::None {
                    // A quick second click on a button counts as a click.
                    click(b, h);
                }
            });
            Some(0)
        }
        wm::WM_RBUTTONUP => {
            // Outside the bar's borrow: the menu runs its own message loop.
            menu(hwnd);
            Some(0)
        }
        wm::WM_ENTERSIZEMOVE => {
            with_bar(|b| b.moving = true);
            Some(0)
        }
        wm::WM_EXITSIZEMOVE => {
            with_bar(|b| {
                b.moving = false;
                let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                wm::GetWindowRect(hwnd, &mut rc);
                b.anchor = Some((rc.right, rc.top));
                send(json!({"t": "moved", "right": rc.right, "y": rc.top}));
                paint(b);
            });
            Some(0)
        }
        wm::WM_DPICHANGED => {
            with_bar(|b| {
                let mut rc = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                wm::GetWindowRect(hwnd, &mut rc);
                b.anchor = Some((rc.right, rc.top));
                b.key.clear();
                paint(b);
            });
            Some(0)
        }
        wm::WM_SETTINGCHANGE | wm::WM_DISPLAYCHANGE => {
            with_bar(|b| {
                b.light = taskbar_is_light();
                paint(b);
            });
            None
        }
        wm::WM_CLOSE => {
            wm::DestroyWindow(hwnd);
            Some(0)
        }
        wm::WM_DESTROY => {
            wm::PostQuitMessage(0);
            Some(0)
        }
        _ => None,
    };
    match handled {
        Some(r) => r,
        None => wm::DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn make_fonts() -> Fonts {
    unsafe fn family(names: &[&str]) -> *mut gp::GpFontFamily {
        for n in names {
            let w = wide(n);
            let mut f = std::ptr::null_mut();
            if gp::GdipCreateFontFamilyFromName(w.as_ptr(), std::ptr::null_mut(), &mut f) == gp::Ok && !f.is_null() {
                return f;
            }
        }
        std::ptr::null_mut()
    }
    let sans = family(&["Segoe UI Variable Text", "Segoe UI", "Tahoma"]);
    let mono = family(&["Cascadia Mono", "Consolas", "Courier New"]);
    let mut task = std::ptr::null_mut();
    gp::GdipCreateFont(sans, 12.5, gp::FontStyleRegular, gp::UnitPixel, &mut task);
    let mut clock = std::ptr::null_mut();
    gp::GdipCreateFont(mono, 11.0, gp::FontStyleBold, gp::UnitPixel, &mut clock);

    let mut fmt_task = std::ptr::null_mut();
    gp::GdipCreateStringFormat(gp::StringFormatFlagsLineLimit, 0, &mut fmt_task);
    gp::GdipSetStringFormatAlign(fmt_task, gp::StringAlignmentFar);
    gp::GdipSetStringFormatLineAlign(fmt_task, gp::StringAlignmentCenter);
    gp::GdipSetStringFormatTrimming(fmt_task, gp::StringTrimmingEllipsisWord);
    let mut fmt_center = std::ptr::null_mut();
    gp::GdipCreateStringFormat(gp::StringFormatFlagsNoWrap | gp::StringFormatFlagsNoClip, 0, &mut fmt_center);
    gp::GdipSetStringFormatAlign(fmt_center, gp::StringAlignmentCenter);
    gp::GdipSetStringFormatLineAlign(fmt_center, gp::StringAlignmentCenter);

    let mut measure = std::ptr::null_mut();
    let screen = gdi::GetDC(std::ptr::null_mut());
    gp::GdipCreateFromHDC(screen, &mut measure);
    gp::GdipSetTextRenderingHint(measure, gp::TextRenderingHintAntiAliasGridFit);
    // The measuring context keeps working after the DC is released; it only reads font metrics.
    Fonts { task, clock, fmt_task, fmt_center, measure }
}

fn main() {
    let (Some(port), Some(token)) = (arg("--port"), arg("--token")) else {
        eprintln!("mazecat-bar is started by Maze Cat itself.");
        return;
    };
    let Ok(stream) = TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap_or(0))) else { return };
    let _ = stream.set_nodelay(true);
    let Ok(reader) = stream.try_clone() else { return };
    *OUT.lock().unwrap() = Some(stream);
    send(json!({"t": "hello", "token": token}));

    unsafe {
        let mut gp_token = 0usize;
        let input = gp::GdiplusStartupInput { GdiplusVersion: 1, DebugEventCallback: 0, SuppressBackgroundThread: 0, SuppressExternalCodecs: 0 };
        gp::GdiplusStartup(&mut gp_token, &input, std::ptr::null_mut());

        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide("MazeCatBar");
        let wc = wm::WNDCLASSEXW {
            cbSize: std::mem::size_of::<wm::WNDCLASSEXW>() as u32,
            style: wm::CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: std::ptr::null_mut(),
            hCursor: wm::LoadCursorW(std::ptr::null_mut(), wm::IDC_ARROW),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        wm::RegisterClassExW(&wc);
        let title = wide("Maze Cat");
        // Not a tool window: it shows in Alt+Tab like any open app. That is what lets
        // Windows keep a uiAccess window above the taskbar (Brian's finding from the first Maze Cat).
        // No WS_EX_NOACTIVATE either: Windows leaves such windows out of Alt+Tab. Clicks still
        // don't take focus (WM_MOUSEACTIVATE answers MA_NOACTIVATE).
        let ex = wm::WS_EX_LAYERED | wm::WS_EX_TOPMOST | wm::WS_EX_APPWINDOW;
        let mut hwnd: HWND = std::ptr::null_mut();
        if has_ui_access() {
            if let Some(f) = user32("CreateWindowInBand") {
                let f: CreateWindowInBandFn = std::mem::transmute(f);
                hwnd = f(ex, class.as_ptr(), title.as_ptr(), wm::WS_POPUP, 0, 0, 1, 1, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null(), ZBID_UIACCESS);
            }
        }
        if hwnd.is_null() {
            hwnd = wm::CreateWindowExW(ex, class.as_ptr(), title.as_ptr(), wm::WS_POPUP, 0, 0, 1, 1, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
        }
        if hwnd.is_null() {
            log("could not create the bar window");
            return;
        }
        log(&status(hwnd));
        HWND_BAR.store(hwnd as isize, Ordering::Relaxed);

        // Borrow the main app's icon for Alt+Tab and the taskbar.
        if let Some(main_exe) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("mazecat.exe"))) {
            let path = wide(&main_exe.to_string_lossy());
            let (mut large, mut small) = (std::ptr::null_mut(), std::ptr::null_mut());
            if ExtractIconExW(path.as_ptr(), 0, &mut large, &mut small, 1) > 0 {
                wm::SendMessageW(hwnd, wm::WM_SETICON, wm::ICON_BIG as WPARAM, large as LPARAM);
                wm::SendMessageW(hwnd, wm::WM_SETICON, wm::ICON_SMALL as WPARAM, small as LPARAM);
            }
        }

        let bar = Bar {
            hwnd,
            timer: Timer::default(),
            def_min: 25,
            anchor: None,
            presets: false,
            hover: Hit::None,
            press: None,
            tracking: false,
            moving: false,
            rang: false,
            shown: false,
            got_state: false,
            light: taskbar_is_light(),
            key: String::new(),
            hits: Vec::new(),
            fonts: make_fonts(),
        };
        BAR.with(|b| *b.borrow_mut() = Some(bar));

        wm::SetTimer(hwnd, TIMER_TICK, 250, None);
        wm::SetTimer(hwnd, TIMER_TOP, 1500, None);
        // Whenever another window comes to the front (the taskbar, Start, any app), stay on top.
        SetWinEventHook(wm::EVENT_SYSTEM_FOREGROUND, wm::EVENT_SYSTEM_FOREGROUND, std::ptr::null_mut(), Some(on_foreground), 0, 0, wm::WINEVENT_OUTOFCONTEXT | wm::WINEVENT_SKIPOWNPROCESS);

        // Messages from the main app arrive on another thread; hand each one to the window.
        let target = hwnd as isize;
        std::thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    let p = Box::into_raw(Box::new(v));
                    if wm::PostMessageW(target as HWND, WM_NET, 0, p as LPARAM) == 0 {
                        drop(Box::from_raw(p));
                    }
                }
            }
            // Maze Cat closed: so does the bar.
            wm::PostMessageW(target as HWND, wm::WM_CLOSE, 0, 0);
        });

        let mut msg: wm::MSG = std::mem::zeroed();
        while wm::GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            wm::TranslateMessage(&msg);
            wm::DispatchMessageW(&msg);
        }
        gp::GdiplusShutdown(gp_token);
    }
}
