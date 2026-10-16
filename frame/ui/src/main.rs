//! Maison on the wall frame, drawn natively: SDL 3 under cage (Wayland, EGL, GLES 2 on Lima),
//! no WebView (ARCHITECTURE.md). The loop sleeps on SDL's events: a finger, a window event, or
//! a thread's word that something came (a read, an answer); it draws only when something
//! changed, and every frame while a fling runs. Idle, the CPU sleeps.
//!
//! The environment (config.rs) says where Maison is, where the cookie is kept, the language,
//! the turn (`MAISON_ROTATE=180`), the scale, a window for a simulation; `SDL_RENDER_DRIVER`
//! (default `opengles2`) and `SDL_RENDER_VSYNC` (default 1) are SDL's own. `MAISON_STATS=1` says
//! the frame times on stdout (the journal); `MAISON_FRAMES=n` quits after n drawings (a
//! measured run); `MAISON_STDIN=1` takes a simulation's commands on stdin (sim.rs).

mod app;
mod backlight;
mod client;
mod commands;
mod config;
mod families;
mod i18n;
mod live;
mod pairing;
mod paths;
mod sim;
#[cfg(test)]
mod testing;
mod theme;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use sdl3::render::{Canvas, Texture, TextureCreator};
use sdl3::video::{Window, WindowContext};

use crate::app::App;
use crate::config::{Config, PANEL};
use crate::i18n::k;
use crate::ui::gfx::{Draw, Error, Gfx};
use crate::ui::layout::{Ctx, Drawn};
use crate::ui::model::Screen;
use crate::ui::screens::paint;

/// Drawings failed in a row before the app gives up (systemd starts it again).
const MAX_FAILURES: u32 = 20;
/// A drawing that failed is tried again this much later.
const RETRY: Duration = Duration::from_secs(1);

/// The threads' word that something came; one in the queue at a time.
struct Wake;

/// Frame times, said every ten seconds while drawing (`MAISON_STATS=1`).
struct Stats {
    since: Instant,
    frames: u32,
    worst: Duration,
    total: Duration,
}

impl Stats {
    fn new() -> Self {
        Self {
            since: Instant::now(),
            frames: 0,
            worst: Duration::ZERO,
            total: Duration::ZERO,
        }
    }

    fn tick(&mut self, took: Duration) {
        self.frames += 1;
        self.worst = self.worst.max(took);
        self.total += took;
        if self.since.elapsed() >= Duration::from_secs(10) {
            println!(
                "{} frames in 10 s, mean {:.1} ms, worst {:.1} ms",
                self.frames,
                self.total.as_secs_f64() * 1000.0 / f64::from(self.frames.max(1)),
                self.worst.as_secs_f64() * 1000.0
            );
            *self = Self::new();
        }
    }
}

/// What the canvas holds now, as a PNG (a simulation's eyes).
fn save_png(canvas: &Canvas<Window>, path: &str) -> Result<(), Error> {
    let surface = canvas
        .read_pixels(None)?
        .convert_format(sdl3::pixels::PixelFormat::RGBA32)?;
    let (w, h, pitch) = (surface.width(), surface.height(), surface.pitch() as usize);
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    surface.with_lock(|px| {
        for row in px.chunks(pitch).take(h as usize) {
            data.extend_from_slice(&row[..w as usize * 4]);
        }
    });
    // opaque pixels: premultiplied or not, the same
    for p in data.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    let size = resvg::tiny_skia::IntSize::from_wh(w, h).ok_or("an empty canvas")?;
    let pm = resvg::tiny_skia::Pixmap::from_vec(data, size).ok_or("a canvas of no size")?;
    pm.save_png(path)?;
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("maison-frame-ui: {e}");
        std::process::exit(1);
    }
}

/// SDL's hints, not the environment: SDL_RENDER_DRIVER set outside still wins. One SDL refuses
/// is said (the app goes on with SDL's default).
fn hints() {
    for (name, value) in [
        ("SDL_RENDER_DRIVER", "opengles2"),
        ("SDL_RENDER_VSYNC", "1"),
        // fingers only: the touch's synthetic mouse is noise, and a simulation's mouse is a finger
        ("SDL_TOUCH_MOUSE_EVENTS", "0"),
        ("SDL_MOUSE_TOUCH_EVENTS", "1"),
    ] {
        // SDL refuses a hint its environment variable already sets: that one wins, unsaid
        if !sdl3::hint::set(name, value) && std::env::var_os(name).is_none() {
            eprintln!("SDL refused the hint {name}={value}");
        }
    }
}

/// What the environment asks of a run besides the config.
struct Run {
    stats: bool,
    /// Quit after this many drawings.
    stop_after: Option<u64>,
    /// A simulation's commands on stdin.
    simulated: bool,
}

impl Run {
    fn from_env() -> Self {
        Self {
            stats: std::env::var("MAISON_STATS").is_ok_and(|v| v == "1"),
            stop_after: std::env::var("MAISON_FRAMES")
                .ok()
                .and_then(|n| n.parse::<u64>().ok()),
            simulated: std::env::var("MAISON_STDIN").is_ok_and(|v| v == "1"),
        }
    }
}

/// The threads' way to wake the loop: one word in SDL's queue at a time; one that could not be
/// queued lets the next one try.
fn waker(events: &sdl3::EventSubsystem, pending: Arc<AtomicBool>) -> Arc<dyn Fn() + Send + Sync> {
    let sender = events.event_sender();
    Arc::new(move || {
        if !pending.swap(true, Ordering::AcqRel)
            && let Err(e) = sender.push_custom_event(Wake)
        {
            pending.store(false, Ordering::Release);
            eprintln!("the loop could not be woken: {e}");
        }
    })
}

fn run() -> Result<(), Error> {
    let started = Instant::now();
    let config = Config::from_env()?;
    hints();
    let opts = Run::from_env();

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let events = sdl.event()?;
    events.register_custom_event::<Wake>()?;
    let ttf = sdl3::ttf::init()?;

    let pending = Arc::new(AtomicBool::new(false));
    let wake = waker(&events, pending.clone());
    let commands = opts.simulated.then(|| sim::listen(wake.clone()));
    let mut app = App::new(&config, wake);

    let title = app.words.s(k::BRANDING_NAME);
    let window = match config.window {
        Some((w, h)) => video.window(&title, w, h).build()?,
        None => video
            .window(&title, PANEL.0, PANEL.1)
            .fullscreen()
            .borderless()
            .build()?,
    };
    // fallible: `into_canvas` panics when the driver has no renderer (GLES missing)
    let mut canvas = sdl3::render::create_renderer(window, None)?;
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    println!(
        "video {}, renderer {}, rotate {}, scale {}",
        video.current_video_driver(),
        canvas.renderer_name,
        if config.rotate { 180 } else { 0 },
        config.scale
    );
    let creator = canvas.texture_creator();
    let gfx = Gfx::new(ttf, &creator)?;
    let mut panel = Panel {
        canvas,
        creator: &creator,
        gfx,
        turned: None,
        rotate: config.rotate,
        scale: config.scale,
    };
    let mut pump = sdl.event_pump()?;
    let mut shot: Option<String> = None;
    let mut said = String::new();
    let mut drawings = 0_u64;
    // a drawing that failed is tried again a little later; past this many in a row, the app ends
    // (systemd starts it again): the renderer is lost
    let mut failures = 0_u32;
    let mut retry: Option<Instant> = None;
    let mut stats = Stats::new();

    loop {
        let first = wait(&mut pump, &app, retry);
        let (w, h) = panel.canvas.output_size()?;
        for event in first.into_iter().chain(pump.poll_iter()) {
            match on_event(&mut app, event, (w as f32, h as f32), &pending) {
                Flow::Quit => return Ok(()),
                Flow::Redraw => panel.turned = None,
                Flow::Go => {}
            }
        }
        if let Some(rx) = &commands
            && let Some(path) = simulation(&mut app, rx)
        {
            shot = Some(path);
        }
        let now = Instant::now();
        if retry.is_some_and(|t| now >= t) {
            retry = None;
            app.dirty = true;
        }
        app.tick(now);
        if !app.dirty {
            continue;
        }
        app.dirty = false;
        let begun = Instant::now();
        let shown = app.screen(now);
        let drawn = match panel.draw(&app, &shown, now, shot.take()) {
            Ok(d) => {
                failures = 0;
                d
            }
            Err(e) => {
                failures += 1;
                eprintln!("a drawing failed ({failures} in a row): {e}");
                if failures >= MAX_FAILURES {
                    return Err(e);
                }
                // the turned picture's texture is made again, the drawing tried in a second
                panel.turned = None;
                retry = Some(Instant::now() + RETRY);
                continue;
            }
        };
        app.drawn(drawn.hits, drawn.content, h as f32);
        if opts.simulated {
            say_screen(&shown, &mut said);
        }
        drawings += 1;
        if drawings == 1 {
            println!(
                "first frame after {:.0} ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        if opts.stats {
            stats.tick(begun.elapsed());
        }
        if opts.stop_after.is_some_and(|n| drawings >= n) {
            return Ok(());
        }
    }
}

/// The first event once there is one, or none when the app is due by itself (a drawing owed,
/// something turning with time, a failed drawing's retry).
fn wait(pump: &mut sdl3::EventPump, app: &App, retry: Option<Instant>) -> Option<Event> {
    if app.dirty {
        return None;
    }
    let now = Instant::now();
    let next = [app.next_wake(now), retry].into_iter().flatten().min();
    match next.map(|t| t.saturating_duration_since(now)) {
        Some(t) => pump.wait_event_timeout(t),
        None => Some(pump.wait_event()),
    }
}

/// What an event asks of the loop.
enum Flow {
    Go,
    /// The window changed: everything drawn again, the turned picture made again.
    Redraw,
    Quit,
}

fn on_event(app: &mut App, event: Event, (wf, hf): (f32, f32), pending: &AtomicBool) -> Flow {
    let now = Instant::now();
    // SDL says fingers in 0..1 of the window. The frame's touch is already the picture's way up
    // (its device tree maps it to what the person sees, as cog showed: it turned its picture,
    // never the touch), whoever turns the picture
    let at = |x: f32, y: f32| (x * wf, y * hf);
    match event {
        Event::Quit { .. }
        | Event::KeyDown {
            keycode: Some(Keycode::Escape),
            ..
        } => return Flow::Quit,
        Event::FingerDown { x, y, .. } => {
            let (x, y) = at(x, y);
            if said_touches() {
                eprintln!("touch down {x:.0},{y:.0} on {}", app.key_under(x, y));
            }
            app.finger_down(x, y, now);
        }
        Event::FingerMotion { x, y, .. } => {
            let (x, y) = at(x, y);
            app.finger_motion(x, y, now);
        }
        Event::FingerUp { x, y, .. } => {
            let (x, y) = at(x, y);
            if said_touches() {
                eprintln!("touch up {x:.0},{y:.0}");
            }
            app.finger_up(x, y, now);
        }
        Event::Window {
            win_event:
                WindowEvent::Exposed | WindowEvent::PixelSizeChanged(..) | WindowEvent::Resized(..),
            ..
        } => {
            app.dirty = true;
            return Flow::Redraw;
        }
        e if e.is_user_event() => {
            // frees the boxed event; the flag lets the next one in
            let _ = e.as_user_event_type::<Wake>();
            pending.store(false, Ordering::Release);
            app.take();
        }
        _ => {}
    }
    Flow::Go
}

/// A simulation's commands since: run, said; a shot asked is returned.
fn simulation(app: &mut App, rx: &Receiver<String>) -> Option<String> {
    let mut shot = None;
    while let Ok(line) = rx.try_recv() {
        match sim::parse(&line).and_then(|c| sim::run(app, c)) {
            Ok(Some(path)) => {
                shot = Some(path);
                app.dirty = true;
            }
            Ok(None) => println!("ok {line}"),
            Err(e) => println!("error {line}: {e}"),
        }
    }
    shot
}

/// Each finger down and up said in the log, where it landed and on what (`MAISON_STATS=1`, as
/// the frame's unit sets it): a tap that does nothing can be read back.
fn said_touches() -> bool {
    static SAID: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SAID.get_or_init(|| std::env::var("MAISON_STATS").is_ok_and(|v| v == "1"))
}

/// What the screen is, said to a simulation when it changes.
fn say_screen(screen: &Screen, said: &mut String) {
    let now_said = match screen {
        Screen::House { .. } => "screen house".to_string(),
        Screen::Page { title, .. } => format!("screen page {title}"),
        Screen::Door { title, code, .. } => match code {
            Some(c) => format!("screen door {title}\ncode {c}"),
            None => format!("screen door {title}"),
        },
    };
    if now_said != *said {
        println!("{now_said}");
        *said = now_said;
    }
}

/// The panel and what draws on it: the canvas, the caches, the turned picture's texture.
struct Panel<'t> {
    canvas: Canvas<Window>,
    creator: &'t TextureCreator<WindowContext>,
    gfx: Gfx<'t>,
    /// The picture drawn before it is turned (`MAISON_ROTATE=180`).
    turned: Option<Texture<'t>>,
    rotate: bool,
    scale: f32,
}

impl Panel<'_> {
    /// One drawing of `shown`, presented (and saved first to `shot`, a simulation's).
    fn draw(
        &mut self,
        app: &App,
        shown: &Screen,
        now: Instant,
        shot: Option<String>,
    ) -> Result<Drawn, Error> {
        let (w, h) = self.canvas.output_size()?;
        self.gfx.begin();
        let toasts = app.toasts();
        let ctx = Ctx {
            pal: app.palette(),
            words: &app.words,
            commands: app.commands(),
            sliders: app.sliders(),
            now,
            scale: self.scale,
            width: w as f32,
            height: h as f32,
            scroll: app.scroll,
        };
        let drawn = if self.rotate {
            let target = match &mut self.turned {
                Some(t) => t,
                None => {
                    let mut t = self.creator.create_texture_target(None, w, h)?;
                    t.set_blend_mode(sdl3::render::BlendMode::None);
                    self.turned.insert(t)
                }
            };
            let gfx = &mut self.gfx;
            let mut out = None;
            self.canvas
                .with_texture_canvas(target, |c: &mut Canvas<Window>| {
                    let mut d = Draw { canvas: c, gfx };
                    out = Some(paint(&mut d, &ctx, shown, &toasts).map_err(|e| e.to_string()));
                })?;
            let drawn = out.ok_or("nothing drawn")??;
            self.canvas
                .copy_ex(&*target, None, None, 180.0, None, false, false)?;
            drawn
        } else {
            let mut d = Draw {
                canvas: &mut self.canvas,
                gfx: &mut self.gfx,
            };
            paint(&mut d, &ctx, shown, &toasts)?
        };
        if let Some(path) = shot {
            save_png(&self.canvas, &path)?;
            println!("shot {path}");
        }
        self.canvas.present();
        Ok(drawn)
    }
}
