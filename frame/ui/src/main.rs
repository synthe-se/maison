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
mod client;
mod commands;
mod config;
mod families;
mod i18n;
mod live;
mod pairing;
mod sim;
#[cfg(test)]
mod testing;
mod theme;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::Keycode;
use sdl3::render::{Canvas, Texture};
use sdl3::video::Window;

use crate::app::App;
use crate::config::Config;
use crate::ui::gfx::{Draw, Error, Gfx};
use crate::ui::paint::{Ctx, paint};

/// Drawings failed in a row before the app gives up (systemd starts it again).
const MAX_FAILURES: u32 = 20;

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

fn run() -> Result<(), Error> {
    let started = Instant::now();
    let config = Config::from_env()?;
    // hints, not the environment: SDL_RENDER_DRIVER set outside still wins
    sdl3::hint::set("SDL_RENDER_DRIVER", "opengles2");
    sdl3::hint::set("SDL_RENDER_VSYNC", "1");
    // fingers only: the touch's synthetic mouse is noise, and a simulation's mouse is a finger
    sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
    sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "1");
    let stats_on = std::env::var("MAISON_STATS").is_ok_and(|v| v == "1");
    let stop_after = std::env::var("MAISON_FRAMES")
        .ok()
        .and_then(|n| n.parse::<u64>().ok());
    let simulated = std::env::var("MAISON_STDIN").is_ok_and(|v| v == "1");

    let sdl = sdl3::init()?;
    let video = sdl.video()?;
    let events = sdl.event()?;
    events.register_custom_event::<Wake>()?;
    let ttf = sdl3::ttf::init()?;
    let window = match config.window {
        Some((w, h)) => video.window("Maison", w, h).build()?,
        None => video
            .window("Maison", 800, 1280)
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
    let mut gfx = Gfx::new(ttf, &creator)?;

    let pending = Arc::new(AtomicBool::new(false));
    let sender = events.event_sender();
    let flag = pending.clone();
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        // one word in the queue at a time; one that could not be queued lets the next one try
        if !flag.swap(true, Ordering::AcqRel)
            && let Err(e) = sender.push_custom_event(Wake)
        {
            flag.store(false, Ordering::Release);
            eprintln!("the loop could not be woken: {e}");
        }
    });
    let commands = simulated.then(|| sim::listen(wake.clone()));
    let mut app = App::new(&config, wake);
    let mut shot: Option<String> = None;
    let mut said = String::new();
    let mut pump = sdl.event_pump()?;
    let mut turned: Option<Texture> = None;
    let mut drawings = 0_u64;
    // a drawing that failed is tried again a little later; past this many in a row, the app ends
    // (systemd starts it again): the renderer is lost
    let mut failures = 0_u32;
    let mut retry: Option<Instant> = None;
    let mut stats = Stats::new();

    loop {
        let now = Instant::now();
        let first_event = if app.dirty {
            None
        } else {
            let next = [app.next_wake(now), retry].into_iter().flatten().min();
            match next.map(|t| t.saturating_duration_since(now)) {
                Some(t) => pump.wait_event_timeout(t),
                None => Some(pump.wait_event()),
            }
        };
        let (w, h) = canvas.output_size()?;
        let (wf, hf) = (w as f32, h as f32);
        // SDL says fingers in 0..1 of the window. The frame's touch is already the picture's
        // way up (its device tree maps it to what the person sees, as cog showed: it turned its
        // picture, never the touch), whoever turns the picture
        let at = |x: f32, y: f32| (x * wf, y * hf);
        for event in first_event.into_iter().chain(pump.poll_iter()) {
            let now = Instant::now();
            match event {
                Event::Quit { .. }
                | Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                Event::FingerDown { x, y, .. } => {
                    let (x, y) = at(x, y);
                    app.finger_down(x, y, now);
                }
                Event::FingerMotion { x, y, .. } => {
                    let (x, y) = at(x, y);
                    app.finger_motion(x, y, now);
                }
                Event::FingerUp { x, y, .. } => {
                    let (x, y) = at(x, y);
                    app.finger_up(x, y, now);
                }
                Event::Window {
                    win_event:
                        WindowEvent::Exposed
                        | WindowEvent::PixelSizeChanged(..)
                        | WindowEvent::Resized(..),
                    ..
                } => {
                    turned = None;
                    app.dirty = true;
                }
                e if e.is_user_event() => {
                    // frees the boxed event; the flag lets the next one in
                    let _ = e.as_user_event_type::<Wake>();
                    pending.store(false, Ordering::Release);
                    app.take();
                }
                _ => {}
            }
        }
        if let Some(rx) = &commands {
            while let Ok(line) = rx.try_recv() {
                match sim::parse(&line).and_then(|c| sim::run(&mut app, c)) {
                    Ok(Some(path)) => {
                        shot = Some(path);
                        app.dirty = true;
                    }
                    Ok(None) => println!("ok {line}"),
                    Err(e) => println!("error {line}: {e}"),
                }
            }
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
        gfx.begin();
        let screen = app.screen(now);
        let toasts = app.toasts();
        let ctx = Ctx {
            pal: app.palette(),
            words: &app.words,
            commands: app.commands(),
            sliders: app.sliders(),
            now,
            scale: config.scale,
            width: wf,
            height: hf,
            scroll: app.scroll,
        };
        let mut draw = || -> Result<ui::paint::Drawn, Error> {
            let drawn = if config.rotate {
                if turned.is_none() {
                    let mut t = creator.create_texture_target(None, w, h)?;
                    t.set_blend_mode(sdl3::render::BlendMode::None);
                    turned = Some(t);
                }
                let target = turned.as_mut().expect("just made");
                let mut out = None;
                canvas.with_texture_canvas(target, |c: &mut Canvas<Window>| {
                    let mut d = Draw {
                        canvas: c,
                        gfx: &mut gfx,
                    };
                    out = Some(paint(&mut d, &ctx, &screen, &toasts).map_err(|e| e.to_string()));
                })?;
                let drawn = out.ok_or("nothing drawn")??;
                canvas.copy_ex(target, None, None, 180.0, None, false, false)?;
                drawn
            } else {
                let mut d = Draw {
                    canvas: &mut canvas,
                    gfx: &mut gfx,
                };
                paint(&mut d, &ctx, &screen, &toasts)?
            };
            if let Some(path) = shot.take() {
                save_png(&canvas, &path)?;
                println!("shot {path}");
            }
            canvas.present();
            Ok(drawn)
        };
        let drawn = match draw() {
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
                turned = None;
                retry = Some(Instant::now() + Duration::from_secs(1));
                continue;
            }
        };
        app.drawn(drawn.hits, drawn.content, hf);
        if simulated {
            let now_said = match &screen {
                ui::model::Screen::House(_) => "screen house".to_string(),
                ui::model::Screen::Door { title, code, .. } => match code {
                    Some(c) => format!("screen door {title}\ncode {c}"),
                    None => format!("screen door {title}"),
                },
            };
            if now_said != said {
                println!("{now_said}");
                said = now_said;
            }
        }
        drawings += 1;
        if drawings == 1 {
            println!(
                "first frame after {:.0} ms",
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        if stats_on {
            stats.tick(begun.elapsed());
        }
        if stop_after.is_some_and(|n| drawings >= n) {
            return Ok(());
        }
    }
}
