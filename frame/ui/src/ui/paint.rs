//! Lays a `Screen` out and draws it, in the web app's measures (tokens.css, app.css: the page
//! margin, a group's head, the tiles' grid, a tile's padding and lines), CSS pixels times the
//! scale. Each control drawn leaves where it is (`hits`), for the finger.

use std::time::Instant;

use crate::commands::{Commands, Order, State, Toast};
use crate::i18n::{Words, k};
use crate::theme::{Palette, Rgba, Type, px, ty};
use crate::ui::gfx::{Draw, Error, Pen};
use crate::ui::model::{Act, Body, Button, Group, Lead, Part, Screen, Slider, Tile};
use crate::ui::slider::Sliders;
use maison_api::tempo::Color;

/// What a finger can land on, in the panel's pixels as drawn (scroll applied).
#[derive(Debug, Clone)]
pub struct Hit {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub what: Target,
}

impl Hit {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Debug, Clone)]
pub enum Target {
    Act(Act),
    /// A slider's band, from x0 to x1.
    Slider {
        slider: Slider,
        x0: f32,
        x1: f32,
    },
}

/// What the painter reads besides the screen.
pub struct Ctx<'a> {
    pub pal: &'static Palette,
    pub words: &'a Words,
    pub commands: &'a Commands,
    pub sliders: &'a Sliders,
    pub now: Instant,
    /// CSS pixels to the panel's.
    pub scale: f32,
    /// The panel, in its pixels.
    pub width: f32,
    pub height: f32,
    /// How far the house's column is scrolled, in the panel's pixels.
    pub scroll: f32,
}

/// What a drawing leaves.
pub struct Drawn {
    pub hits: Vec<Hit>,
    /// The house's column, in the panel's pixels (what scrolls).
    pub content: f32,
}

const GROUP_HEAD: f32 = px::CONTROL_H;
/// A finger's controls grow to the 44 px target (tokens.css `pointer: coarse`).
const TOUCH: f32 = px::CONTROL_H;
const ICON: f32 = 18.0;
const GLYPH_ICON: f32 = 22.0;
/// A tile's column at least this wide (app.css `.tiles`: 20rem).
const TILE_MIN: f32 = 320.0;
/// A tile's name (`.tile-title`: the body, bold).
const TITLE: Type = ty::BODY.weight(600);
/// The ring of a gesture's disc, of a swatch.
const RING: f32 = 1.5;

struct P<'d, 'a, 't, 'c> {
    d: &'d mut Draw<'a, 't>,
    c: &'c Ctx<'c>,
    hits: Vec<Hit>,
    /// Added to every y: the column's scroll (negative), or 0 for what stays put.
    dy: f32,
}

/// The layout's measures for the panel's width, in CSS pixels.
pub fn margin(width_css: f32) -> f32 {
    if width_css >= 600.0 {
        24.0
    } else {
        px::PAGE_MARGIN
    }
}

/// How many tiles side by side in `inner` CSS pixels (`repeat(auto-fill, minmax(20rem, 1fr))`).
pub fn columns(inner: f32) -> usize {
    (((inner + px::S_5) / (TILE_MIN + px::S_5)).floor() as usize).max(1)
}

impl P<'_, '_, '_, '_> {
    fn s(&self) -> f32 {
        self.c.scale
    }

    /// CSS px → the panel's, rounded (crisp edges).
    fn d(&self, v: f32) -> i32 {
        (v * self.s()).round() as i32
    }

    fn y(&self, v: f32) -> i32 {
        (v * self.s() + self.dy).round() as i32
    }

    /// Whether (y, h) in CSS px is on the panel at all.
    fn visible(&self, y: f32, h: f32) -> bool {
        let top = y * self.s() + self.dy;
        top + h * self.s() >= 0.0 && top <= self.c.height
    }

    fn size(&self, t: Type) -> u16 {
        (t.size * self.s()).round().max(1.0) as u16
    }

    fn rounded(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        color: Rgba,
    ) -> Result<(), Error> {
        if !self.visible(y, h) {
            return Ok(());
        }
        let (x0, y0, x1, y1) = (self.d(x), self.y(y), self.d(x + w), self.y(y + h));
        let r = self.d(r);
        self.d.rounded(x0, y0, x1 - x0, y1 - y0, r, color)
    }

    /// A box with a 1 px line around (a tile, a button): `(x, y, w, h)` in CSS px.
    fn framed(
        &mut self,
        (x, y, w, h): (f32, f32, f32, f32),
        r: f32,
        fill: Rgba,
        line: Rgba,
    ) -> Result<(), Error> {
        if !self.visible(y, h) {
            return Ok(());
        }
        let (x0, y0, x1, y1) = (self.d(x), self.y(y), self.d(x + w), self.y(y + h));
        let r = self.d(r);
        let t = (self.s().round() as i32).max(1);
        self.d.rounded(x0, y0, x1 - x0, y1 - y0, r, line)?;
        self.d.rounded(
            x0 + t,
            y0 + t,
            x1 - x0 - 2 * t,
            y1 - y0 - 2 * t,
            (r - t).max(0),
            fill,
        )
    }

    /// A disc of diameter `d` at (x, y); with `ring`, a ring of that width in `ring.1` around.
    fn disc(
        &mut self,
        x: f32,
        y: f32,
        d: f32,
        fill: Rgba,
        ring: Option<(f32, Rgba)>,
    ) -> Result<(), Error> {
        if !self.visible(y, d) {
            return Ok(());
        }
        let (x0, y0, size) = (self.d(x), self.y(y), self.d(d));
        match ring {
            Some((w, color)) => {
                let t = ((w * self.s()).round() as i32).max(1);
                self.d.rounded(x0, y0, size, size, size / 2, color)?;
                self.d.rounded(
                    x0 + t,
                    y0 + t,
                    size - 2 * t,
                    size - 2 * t,
                    size / 2 - t,
                    fill,
                )
            }
            None => self.d.rounded(x0, y0, size, size, size / 2, fill),
        }
    }

    fn icon(
        &mut self,
        name: &'static str,
        x: f32,
        y: f32,
        size: f32,
        color: Rgba,
    ) -> Result<(), Error> {
        if !self.visible(y, size) {
            return Ok(());
        }
        let px = self.d(size).max(1) as u32;
        let (x, y) = (self.d(x), self.y(y));
        self.d.icon(name, x, y, px, color)
    }

    /// One line of `t` in a line box at (x, y), cut to `max` CSS px; its width in CSS px.
    fn line(
        &mut self,
        text: &str,
        t: Type,
        color: Rgba,
        x: f32,
        y: f32,
        max: f32,
    ) -> Result<f32, Error> {
        let size = self.size(t);
        let max_px = (max * self.s()).max(0.0) as u32;
        let fitted = self.d.gfx.fit(t.style(), size, text, max_px)?;
        let (w, h) = self.d.gfx.measure(t.style(), size, 0, &fitted)?;
        if self.visible(y, t.line) {
            let top = y * self.s() + self.dy + (t.line * self.s() - h as f32) / 2.0;
            let pen = Pen {
                style: t.style(),
                size,
                color,
                wrap: 0,
            };
            self.d.text(pen, &fitted, x * self.s(), top)?;
        }
        Ok(w as f32 / self.s())
    }

    /// A line's width in CSS px, uncut.
    fn width(&mut self, text: &str, t: Type) -> Result<f32, Error> {
        let size = self.size(t);
        Ok(self.d.gfx.measure(t.style(), size, 0, text)?.0 as f32 / self.s())
    }

    /// Words wrapped in `w` CSS px; their height in CSS px (whole lines).
    fn para_height(&mut self, text: &str, t: Type, w: f32) -> Result<f32, Error> {
        let size = self.size(t);
        let (_, h) = self
            .d
            .gfx
            .measure(t.style(), size, (w * self.s()) as i32, text)?;
        let font_line = self.d.gfx.measure(t.style(), size, 0, "Hg")?.1.max(1);
        let lines = (h as f32 / font_line as f32).round().max(1.0);
        Ok(lines * t.line)
    }

    fn para(
        &mut self,
        text: &str,
        t: Type,
        color: Rgba,
        x: f32,
        y: f32,
        w: f32,
    ) -> Result<f32, Error> {
        let h = self.para_height(text, t, w)?;
        if self.visible(y, h) {
            let size = self.size(t);
            let font_line = self.d.gfx.measure(t.style(), size, 0, "Hg")?.1 as f32;
            let top = y * self.s() + self.dy + (t.line * self.s() - font_line) / 2.0;
            let pen = Pen {
                style: t.style(),
                size,
                color,
                wrap: (w * self.s()) as i32,
            };
            self.d.text(pen, text, x * self.s(), top)?;
        }
        Ok(h)
    }

    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, what: Target) {
        if self.visible(y, h) {
            let s = self.s();
            self.hits.push(Hit {
                x: x * s,
                y: y * s + self.dy,
                w: w * s,
                h: h * s,
                what,
            });
        }
    }

    // ── bricks ──

    /// A button's width for its content.
    fn button_width(&mut self, b: &Button) -> Result<f32, Error> {
        let label = self.width(&b.label, ty::LABEL)?;
        let icon = if b.icon.is_some() {
            ICON + if b.label.is_empty() { 0.0 } else { px::S_2 }
        } else {
            0.0
        };
        Ok(px::S_3 * 2.0 + icon + label)
    }

    fn busy(&self, b: &Button) -> bool {
        b.key()
            .is_some_and(|k| matches!(self.c.commands.state(k), State::Pending { .. }))
    }

    /// A button in (x, y, w, h); its words dropped when they do not fit (`.btn-row`).
    fn button(&mut self, b: &Button, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let busy = self.busy(b);
        let fade = |c: Rgba| {
            if b.unavailable {
                c.faded(px::DISABLED_OPACITY)
            } else {
                c
            }
        };
        let (fill, line, ink) = if b.primary {
            (pal.accent, pal.accent, pal.on_accent)
        } else if b.ghost {
            (Rgba(0, 0, 0, 0), Rgba(0, 0, 0, 0), pal.ink)
        } else {
            (pal.surface, pal.line, pal.ink)
        };
        self.framed((x, y, w, h), px::RADIUS, fade(fill), fade(line))?;
        let icon = if busy { Some("loader-circle") } else { b.icon };
        let label_w = self.width(&b.label, ty::LABEL)?;
        let icon_w = if icon.is_some() { ICON } else { 0.0 };
        let gap = if icon.is_some() && !b.label.is_empty() {
            px::S_2
        } else {
            0.0
        };
        let words =
            !b.label.is_empty() && icon_w + gap + label_w <= w - px::S_3 * 2.0 || icon.is_none();
        let content = icon_w
            + if words {
                gap + label_w.min(w - px::S_3 * 2.0 - icon_w - gap)
            } else {
                0.0
            };
        let mut cx = x + (w - content) / 2.0;
        if let Some(name) = icon {
            self.icon(name, cx, y + (h - ICON) / 2.0, ICON, fade(ink))?;
            cx += ICON + gap;
        }
        if words {
            let room = x + w - px::S_3 - cx;
            self.line(
                &b.label,
                ty::LABEL,
                fade(ink),
                cx,
                y + (h - ty::LABEL.line) / 2.0,
                room,
            )?;
        }
        if !b.unavailable && !busy {
            self.hit(x, y, w, h, Target::Act(b.act.clone()));
        }
        Ok(())
    }

    /// Buttons left to right, wrapping (`.scenes`, `.actions`); the height used.
    fn flow(
        &mut self,
        buttons: &[Button],
        x: f32,
        y: f32,
        w: f32,
        draw: bool,
    ) -> Result<f32, Error> {
        let (mut cx, mut cy) = (x, y);
        for b in buttons {
            let bw = self.button_width(b)?.min(w);
            if cx > x && cx + bw > x + w {
                cx = x;
                cy += TOUCH + px::S_2;
            }
            if draw {
                self.button(b, cx, cy, bw, TOUCH)?;
            }
            cx += bw + px::S_2;
        }
        Ok(if buttons.is_empty() {
            0.0
        } else {
            cy - y + TOUCH
        })
    }

    fn slider_height(s: &Slider) -> f32 {
        px::CONTROL_H
            + if s.ends.is_some() {
                px::S_2 + ty::META.line
            } else {
                0.0
            }
    }

    fn slider(&mut self, s: &Slider, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let value = self.c.sliders.shown(s, self.c.now);
        let share =
            |v: f64| ((v - s.min) / (s.max - s.min).max(f64::EPSILON)).clamp(0.0, 1.0) as f32;
        // the thumb's centre travels between half a thumb from each end
        let (x0, x1) = (x + px::THUMB / 2.0, x + w - px::THUMB / 2.0);
        let ty0 = y + (px::CONTROL_H - px::TRACK_H) / 2.0;
        self.rounded(x, ty0, w, px::TRACK_H, px::RADIUS_PILL, pal.ground_raised)?;
        let at = x0 + (x1 - x0) * share(value);
        self.rounded(x, ty0, at - x, px::TRACK_H, px::RADIUS_PILL, pal.accent)?;
        if let Some(m) = s.mark {
            let mx = x0 + (x1 - x0) * share(m);
            self.rounded(
                mx - px::S_HALF / 2.0,
                ty0,
                px::S_HALF,
                px::TRACK_H,
                0.0,
                pal.ink,
            )?;
        }
        self.disc(
            at - px::THUMB / 2.0,
            y + (px::CONTROL_H - px::THUMB) / 2.0,
            px::THUMB,
            pal.surface,
            Some((2.0, pal.accent)),
        )?;
        if let Some((left, right)) = &s.ends {
            let ey = y + px::CONTROL_H + px::S_2;
            self.line(left, ty::META, pal.ink_muted, x, ey, w / 2.0)?;
            let rw = self.width(right, ty::META)?;
            self.line(right, ty::META, pal.ink_muted, x + w - rw, ey, w / 2.0)?;
        }
        self.hit(
            x,
            y,
            w,
            px::CONTROL_H,
            Target::Slider {
                slider: s.clone(),
                x0: x0 * self.s(),
                x1: x1 * self.s(),
            },
        );
        Ok(())
    }

    // ── a tile ──

    fn part_height(&mut self, part: &Part, w: f32) -> Result<f32, Error> {
        Ok(match part {
            Part::Slider(s) => Self::slider_height(s),
            Part::Row(_) => TOUCH,
            Part::Hint(t) => self.para_height(t, ty::SECONDARY, w)?,
            Part::Note { .. } => TOUCH,
        })
    }

    /// The fact's width and the title's column, for a tile `w` wide.
    fn head_columns(&mut self, t: &Tile, w: f32) -> Result<(f32, f32), Error> {
        let inner = w - 2.0 * px::S_4;
        let fact_w = match &t.fact {
            Some(f) => self.width(f, ty::META)?.min(inner / 3.0),
            None => 0.0,
        };
        let tw =
            inner - px::TILE_ICON - px::S_3 - if fact_w > 0.0 { fact_w + px::S_3 } else { 0.0 };
        Ok((fact_w, tw))
    }

    /// The head's height: the glyph's, or the name (wrapping, `.tile-title`) and its state line.
    fn head_height(&mut self, t: &Tile, w: f32) -> Result<f32, Error> {
        let (_, tw) = self.head_columns(t, w)?;
        let name_h = self.para_height(&t.name, TITLE, tw)?;
        Ok(px::TILE_ICON.max(name_h + ty::SECONDARY.line))
    }

    fn tile_height(&mut self, t: &Tile, w: f32) -> Result<f32, Error> {
        let inner = w - 2.0 * px::S_4;
        let mut h = 2.0 * px::S_4 + self.head_height(t, w)?;
        for p in &t.parts {
            h += px::S_4 + self.part_height(p, inner)?;
        }
        Ok(h)
    }

    /// The tile's state line, as its gesture says it (DeviceTile's `line`): « Pas de réponse »
    /// once late (and « Réessayer »), « Allumage… » when slow, else the device's words.
    fn state_line(&self, t: &Tile) -> (String, bool, Option<Order>) {
        let w = self.c.words;
        if let Lead::Toggle {
            turn_on,
            turn_off,
            on,
            offline,
            ..
        } = &t.lead
        {
            match self.c.commands.state(&turn_on.key) {
                // « Réessayer »: to the other side of what the device said
                State::Late if !offline => {
                    let retry = if *on { turn_off } else { turn_on };
                    return (
                        w.s(k::COMMAND_NO_ANSWER_SHORT),
                        true,
                        Some(Order::clone(retry)),
                    );
                }
                State::Late => return (w.s(k::COMMAND_NO_ANSWER_SHORT), true, None),
                State::Pending { target, slow: true } => {
                    let key = if target == Some(false) {
                        k::COMMAND_TURNING_OFF
                    } else {
                        k::COMMAND_TURNING_ON
                    };
                    return (w.s(key), t.warn, None);
                }
                _ => {}
            }
        }
        // unreachable: said in the warning color (DeviceTile's `warn || offline`)
        let offline = matches!(t.lead, Lead::Toggle { offline: true, .. });
        (t.state.clone(), t.warn || offline, None)
    }

    fn lead(&mut self, lead: &Lead, x: f32, y: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let size = px::TILE_ICON;
        let inset = (size - GLYPH_ICON) / 2.0;
        match lead {
            Lead::Toggle {
                icon,
                on,
                offline,
                turn_on,
                turn_off,
            } => {
                let state = self.c.commands.state(&turn_on.key);
                let shown = self.c.commands.shown(&turn_on.key, *on) && !offline;
                if let State::Pending { slow: true, .. } = state {
                    // the slow ring: a halo in the accent wash (DeviceTile's `ring`)
                    self.disc(
                        x - px::S_1,
                        y - px::S_1,
                        size + 2.0 * px::S_1,
                        pal.accent_wash,
                        None,
                    )?;
                }
                let (fill, ring, ink) = if *offline {
                    (pal.surface, pal.status_down, pal.status_down)
                } else if shown {
                    (pal.accent, pal.accent, pal.on_accent)
                } else if matches!(state, State::Pending { .. }) {
                    (pal.surface, pal.accent, pal.ink_muted)
                } else {
                    (pal.surface, pal.ink_muted, pal.ink_muted)
                };
                self.disc(x, y, size, fill, Some((RING, ring)))?;
                self.icon(icon, x + inset, y + inset, GLYPH_ICON, ink)?;
                if !offline {
                    let order = if shown { turn_off } else { turn_on };
                    self.hit(
                        x,
                        y,
                        size,
                        size,
                        Target::Act(Act::Order(Order::clone(order))),
                    );
                }
            }
            Lead::Glyph { icon, on } => {
                let (fill, ink) = if *on {
                    (pal.accent_wash, pal.accent)
                } else {
                    (pal.ground_raised, pal.ink_muted)
                };
                self.rounded(x, y, size, size, px::RADIUS_M, fill)?;
                self.icon(icon, x + inset, y + inset, GLYPH_ICON, ink)?;
            }
            Lead::Swatch(color) => self.swatch(*color, x, y, size)?,
        }
        Ok(())
    }

    /// Swatch.svelte at its tile size: blue filled, white a ring, red filled and hatched at
    /// 45° towards the ink, unknown a dashed muted ring; a 2 px ring in its color.
    fn swatch(&mut self, color: Option<Color>, x: f32, y: f32, size: f32) -> Result<(), Error> {
        if !self.visible(y, size) {
            return Ok(());
        }
        let pal = self.c.pal;
        let hex = |c: Rgba| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
        let alpha = |c: Rgba| f32::from(c.3) / 255.0;
        // in a 48 box, the web's 2 px ring drawn inside
        let (ring, fill) = match color {
            Some(Color::Blue) => (pal.tempo_blue, format!(r#"fill="{}""#, hex(pal.tempo_blue))),
            Some(Color::White) => (
                pal.tempo_white_ring,
                format!(
                    r#"fill="{}" fill-opacity="{}""#,
                    hex(pal.tempo_white),
                    alpha(pal.tempo_white)
                ),
            ),
            Some(Color::Red) => (pal.tempo_red, r#"fill="url(#h)""#.to_string()),
            None => (pal.ink_muted, r#"fill="none""#.to_string()),
        };
        let mix = |a: Rgba, b: Rgba, t: f32| {
            let m = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
            Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), 255)
        };
        let hatch = hex(mix(pal.tempo_red, pal.ink, 0.7));
        let dash = if color.is_none() {
            r#" stroke-dasharray="4 3""#
        } else {
            ""
        };
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><defs><pattern id="h" width="6" height="6" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="3" height="6" fill="{red}"/><rect x="3" width="3" height="6" fill="{hatch}"/></pattern></defs><circle cx="24" cy="24" r="23" {fill} stroke="{ring}" stroke-width="2"{dash}/></svg>"##,
            red = hex(pal.tempo_red),
            ring = hex(ring),
        );
        let (px_, py, pw) = (self.d(x), self.y(y), self.d(size).max(1) as u32);
        self.d.picture(&svg, px_, py, pw, pw)
    }

    fn tile(&mut self, t: &Tile, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        self.framed((x, y, w, h), px::RADIUS_L, pal.surface, pal.line)?;
        let (ix, iy) = (x + px::S_4, y + px::S_4);
        let inner = w - 2.0 * px::S_4;
        let (fact_w, tw) = self.head_columns(t, w)?;
        let head_h = self.head_height(t, w)?;
        // the head's row centres its items (`.tile-head`: align-items center)
        self.lead(&t.lead, ix, iy + (head_h - px::TILE_ICON) / 2.0)?;
        let tx = ix + px::TILE_ICON + px::S_3;
        let name_h = self.para_height(&t.name, TITLE, tw)?;
        let text_h = name_h + ty::SECONDARY.line;
        let ty0 = iy + (head_h - text_h) / 2.0;
        self.para(&t.name, TITLE, pal.ink, tx, ty0, tw)?;
        let (state, warn, retry) = self.state_line(t);
        let sy = ty0 + name_h;
        let color = if warn { pal.warn_text } else { pal.ink_muted };
        let used = self.line(&state, ty::SECONDARY, color, tx, sy, tw)?;
        if let Some(order) = retry {
            let sep = " · ";
            let sep_w = self.line(sep, ty::SECONDARY, color, tx + used, sy, tw - used)?;
            let label = self.c.words.s(k::COMMON_RETRY);
            let rx = tx + used + sep_w;
            let rw = self.line(&label, ty::SECONDARY, pal.accent, rx, sy, tw - used - sep_w)?;
            self.rounded(rx, sy + ty::SECONDARY.line - 2.0, rw, 1.0, 0.0, pal.accent)?;
            // a 44 px target around the word
            let pad = (TOUCH - ty::SECONDARY.line) / 2.0;
            self.hit(
                rx - px::S_2,
                sy - pad,
                rw + px::S_4,
                TOUCH,
                Target::Act(Act::Order(order)),
            );
        }
        if let Some(f) = &t.fact {
            let fx = ix + inner - fact_w;
            self.line(
                f,
                ty::META,
                pal.ink_muted,
                fx,
                iy + (head_h - ty::META.line) / 2.0,
                fact_w,
            )?;
        }
        let mut py = iy + head_h;
        for p in &t.parts {
            py += px::S_4;
            let ph = self.part_height(p, inner)?;
            if py > y + h {
                break;
            }
            match p {
                Part::Slider(s) => self.slider(s, ix, py, inner)?,
                Part::Row(buttons) => {
                    let n = buttons.len().max(1) as f32;
                    let bw = (inner - px::S_2 * (n - 1.0)) / n;
                    for (i, b) in buttons.iter().enumerate() {
                        self.button(b, ix + i as f32 * (bw + px::S_2), py, bw, TOUCH)?;
                    }
                }
                Part::Hint(text) => {
                    self.para(text, ty::SECONDARY, pal.ink_muted, ix, py, inner)?;
                }
                Part::Note {
                    text,
                    link,
                    skipped,
                } => {
                    let ly = py + (TOUCH - ty::SECONDARY.line) / 2.0;
                    let color = if *skipped {
                        pal.warn_text
                    } else {
                        pal.ink_muted
                    };
                    let used = self.line(text, ty::SECONDARY, color, ix, ly, inner)?;
                    let sep_w = self.line(
                        " · ",
                        ty::SECONDARY,
                        pal.ink_muted,
                        ix + used,
                        ly,
                        inner - used,
                    )?;
                    let lx = ix + used + sep_w;
                    let busy = self.busy(link);
                    let ink = if busy { pal.ink_muted } else { pal.accent };
                    let lw = self.line(
                        &link.label,
                        ty::SECONDARY,
                        ink,
                        lx,
                        ly,
                        inner - used - sep_w,
                    )?;
                    self.rounded(lx, ly + ty::SECONDARY.line - 2.0, lw, 1.0, 0.0, ink)?;
                    if !busy {
                        self.hit(
                            lx - px::S_2,
                            py,
                            lw + px::S_4,
                            TOUCH,
                            Target::Act(link.act.clone()),
                        );
                    }
                }
            }
            py += ph;
        }
        Ok(())
    }

    // ── a group ──

    fn group(&mut self, g: &Group, x: f32, y: f32, w: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        // the head: the title, then at its end the fact and the actions
        let mut end = x + w;
        let mut actions_w = 0.0;
        for b in g.actions.iter().rev() {
            actions_w += self.button_width(b)? + if actions_w > 0.0 { px::S_2 } else { 0.0 };
        }
        if actions_w > 0.0 {
            let mut bx = end - actions_w;
            for b in &g.actions {
                let bw = self.button_width(b)?;
                self.button(b, bx, y, bw, TOUCH)?;
                bx += bw + px::S_2;
            }
            end -= actions_w + px::S_3;
        }
        if let Some(f) = &g.fact {
            let fw = self.width(f, ty::META)?.min((end - x) / 2.0);
            self.line(
                f,
                ty::META,
                pal.ink_muted,
                end - fw,
                y + (GROUP_HEAD - ty::META.line) / 2.0,
                fw,
            )?;
            end -= fw + px::S_3;
        }
        self.line(
            &g.title,
            ty::GROUP,
            pal.ink,
            x,
            y + (GROUP_HEAD - ty::GROUP.line) / 2.0,
            end - x,
        )?;
        let mut cy = y + GROUP_HEAD + px::S_1;
        if let Some((said, retry)) = &g.stale {
            cy += px::S_3;
            self.icon("clock", x, cy + (TOUCH - 14.0) / 2.0, 14.0, pal.ink_muted)?;
            let tx = x + 14.0 + px::S_1;
            let ly = cy + (TOUCH - ty::SECONDARY.line) / 2.0;
            let used = self.line(
                said,
                ty::SECONDARY,
                pal.ink_muted,
                tx,
                ly,
                w - 14.0 - px::S_1,
            )?;
            let sep_w = self.line(" · ", ty::SECONDARY, pal.ink_muted, tx + used, ly, w)?;
            let lx = tx + used + sep_w;
            let lw = self.line(&retry.label, ty::SECONDARY, pal.accent, lx, ly, x + w - lx)?;
            self.rounded(lx, ly + ty::SECONDARY.line - 2.0, lw, 1.0, 0.0, pal.accent)?;
            self.hit(
                lx - px::S_2,
                cy,
                lw + px::S_4,
                TOUCH,
                Target::Act(retry.act.clone()),
            );
            cy += TOUCH;
        }
        cy += px::S_3;
        match &g.body {
            Body::Loading => {
                // a tile's place, held (Loaded's skeleton)
                let cols = columns(w);
                let tw = (w - px::S_5 * (cols as f32 - 1.0)) / cols as f32;
                let th = 2.0 * px::S_4 + px::TILE_ICON;
                self.rounded(x, cy, tw, th, px::RADIUS_L, pal.ground_raised)?;
                cy += th;
            }
            Body::Failed { text, retry } => {
                cy += self.para(text, ty::BODY, pal.warn_text, x, cy, w)?;
                cy += px::S_3;
                let bw = self.button_width(retry)?;
                self.button(retry, x, cy, bw, TOUCH)?;
                cy += TOUCH;
            }
            Body::Empty { text, hint } => {
                cy += self.para(text, ty::BODY, pal.ink, x, cy, w)?;
                cy += self.para(hint, ty::SECONDARY, pal.ink_muted, x, cy, w)?;
            }
            Body::Buttons(buttons) => {
                let h = self.flow(buttons, x, cy, w, true)?;
                cy += h;
            }
            Body::Tiles(tiles) => {
                let cols = columns(w);
                let tw = (w - px::S_5 * (cols as f32 - 1.0)) / cols as f32;
                for row in tiles.chunks(cols) {
                    let mut rh: f32 = 0.0;
                    for t in row {
                        rh = rh.max(self.tile_height(t, tw)?);
                    }
                    if self.visible(cy, rh) {
                        for (i, t) in row.iter().enumerate() {
                            let th = self.tile_height(t, tw)?;
                            self.tile(t, x + i as f32 * (tw + px::S_5), cy, tw, th)?;
                        }
                    }
                    cy += rh + px::S_5;
                }
                if !tiles.is_empty() {
                    cy -= px::S_5;
                }
            }
        }
        Ok(cy - y)
    }

    // ── the door ──

    fn door(
        &mut self,
        title: &str,
        lead: &[String],
        code: Option<&str>,
        hint: Option<&str>,
        button: Option<&Button>,
        warn: bool,
    ) -> Result<(), Error> {
        let pal = self.c.pal;
        let wcss = self.c.width / self.s();
        let hcss = self.c.height / self.s();
        let m = margin(wcss) * 2.0;
        let w = (wcss - 2.0 * m).min(px::MEASURE);
        let x = (wcss - w) / 2.0;
        // measured first, then drawn centred on the panel's height
        let brand = self.c.words.s(k::BRANDING_NAME);
        let mut h = ty::BRAND_DOOR.line + px::S_5 + self.para_height(title, ty::PAGE, w)?;
        for p in lead {
            h += px::S_4 + self.para_height(p, ty::BODY, w)?;
        }
        if code.is_some() {
            h += px::S_5 + ty::CODE.line;
        }
        if let Some(t) = hint {
            h += px::S_4 + self.para_height(t, ty::SECONDARY, w)?;
        }
        if button.is_some() {
            h += px::S_5 + px::CONTROL_H_L;
        }
        let mut y = ((hcss - h) / 2.0).max(m);
        self.line(&brand, ty::BRAND_DOOR, pal.ink, x, y, w)?;
        y += ty::BRAND_DOOR.line + px::S_5;
        y += self.para(title, ty::PAGE, pal.ink, x, y, w)?;
        for p in lead {
            y += px::S_4;
            y += self.para(
                p,
                ty::BODY,
                if warn { pal.warn_text } else { pal.ink },
                x,
                y,
                w,
            )?;
        }
        if let Some(code) = code {
            y += px::S_5;
            // read across the room: its characters apart (letter-spacing 0.15em)
            let spacing = ty::CODE.size * 0.15;
            let mut cx = x;
            for ch in code.chars() {
                let s = ch.to_string();
                let cw = self.line(&s, ty::CODE, pal.accent, cx, y, w)?;
                cx += cw + spacing;
            }
            y += ty::CODE.line;
        }
        if let Some(t) = hint {
            y += px::S_4;
            y += self.para(t, ty::SECONDARY, pal.ink_muted, x, y, w)?;
        }
        if let Some(b) = button {
            y += px::S_5;
            let bw = self.button_width(b)? + px::S_4;
            self.button(b, x, y, bw, px::CONTROL_H_L)?;
        }
        Ok(())
    }
}

/// Draws the screen; what a finger can land on, and the column's height.
pub fn paint(d: &mut Draw, c: &Ctx, screen: &Screen, toasts: &[Toast]) -> Result<Drawn, Error> {
    let pal = c.pal;
    let (w, h) = (c.width.round() as i32, c.height.round() as i32);
    d.fill(0, 0, w, h, pal.ground)?;
    let mut p = P {
        d,
        c,
        hits: Vec::new(),
        dy: -c.scroll,
    };
    let wcss = c.width / c.scale;
    let m = margin(wcss);
    let mut content = 0.0;
    match screen {
        Screen::House(groups) => {
            let mut y = px::HEADER_H + px::S_5;
            for (i, g) in groups.iter().enumerate() {
                if i > 0 {
                    y += px::S_5;
                }
                y += p.group(g, m, y, wcss - 2.0 * m)?;
            }
            content = (y + px::S_6) * c.scale;
            // the header stays: the brand, a line under it
            p.dy = 0.0;
            p.rounded(0.0, 0.0, wcss, px::HEADER_H, 0.0, pal.ground)?;
            p.rounded(
                0.0,
                px::HEADER_H - 1.0 / c.scale,
                wcss,
                1.0 / c.scale,
                0.0,
                pal.line,
            )?;
            let brand = c.words.s(k::BRANDING_NAME);
            p.line(
                &brand,
                ty::BRAND,
                pal.ink,
                m,
                (px::HEADER_H - ty::BRAND.line) / 2.0,
                wcss - 2.0 * m,
            )?;
        }
        Screen::Door {
            title,
            lead,
            code,
            hint,
            button,
            warn,
        } => {
            p.dy = 0.0;
            p.door(
                title,
                lead,
                code.as_deref(),
                hint.as_deref(),
                button.as_deref(),
                *warn,
            )?;
        }
    }
    // the toasts, at the bottom, above everything (app.css `.toasts`)
    p.dy = 0.0;
    let tw = (wcss - px::S_6).min(px::TOAST_W);
    let mut ty_ = c.height / c.scale - px::S_4;
    for t in toasts.iter().rev() {
        let text_w = tw - px::S_4 - px::S_3;
        let th = p.para_height(&t.text, ty::BODY, text_w)? + 2.0 * px::S_2;
        let th = th.max(TOUCH);
        ty_ -= th;
        let x = (wcss - tw) / 2.0;
        p.rounded(x, ty_, tw, th, px::RADIUS_L, pal.ink)?;
        if t.warn {
            p.rounded(x, ty_, px::S_1, th, 0.0, pal.status_degraded)?;
        }
        let text_h = p.para_height(&t.text, ty::BODY, text_w)?;
        p.para(
            &t.text,
            ty::BODY,
            pal.ground,
            x + px::S_4,
            ty_ + (th - text_h) / 2.0,
            text_w,
        )?;
        ty_ -= px::S_2;
    }
    Ok(Drawn {
        hits: p.hits,
        content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_is_the_web_apps() {
        // 800 px wide: 24 px margins, two columns of 20rem and more
        assert_eq!(margin(800.0), 24.0);
        assert_eq!(margin(400.0), 16.0);
        assert_eq!(columns(752.0), 2);
        assert_eq!(columns(320.0), 1);
        assert_eq!(columns(200.0), 1);
        assert_eq!(columns(1300.0), 3);
    }
}
