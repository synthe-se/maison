//! The web app's bricks, drawn: a button (`.btn`), a row of them that wraps, a slider (Range),
//! a tile (DeviceTile) with its lead (a gesture, a glyph, a Tempo swatch) and its parts, a
//! remote's pad, a month of Tempo days. What cannot act now keeps its hit (`P::cover`): a finger
//! on it does nothing, and never falls through to the row under it.

use crate::commands::{Order, State};
use crate::i18n::k;
use crate::theme::{DARK, Palette, Rgba, hatch, px, ty};
use crate::ui::gfx::Error;
use crate::ui::icon;
use crate::ui::layout::{
    CELL_GAP, FACT_H, GLYPH_ICON, ICON, P, PAD_H, PAD_W, PART_GAP, RING, ROW_PAD_BOTTOM, ROW_PAD_X,
    ROW_PAD_Y, SWATCH_RING, THUMB_RING, TITLE, TOUCH, Target,
};
use crate::ui::model::{Act, Button, Day, Lead, Month, Pad, Part, Slider, Tile};
use maison_api::tempo::Color;
use std::time::Duration;

/// A tile's head, for a tile of one width: the fact's width, the name's column and height, its
/// height whole.
struct HeadBox {
    fact_w: f32,
    name_w: f32,
    name_h: f32,
    h: f32,
}

/// How a Tempo day is marked (Swatch.svelte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    color: Option<Color>,
    /// Not published: a dashed outline over a wash.
    forecast: bool,
    /// A forecast under 60 %: dotted, no wash.
    unsure: bool,
    /// A calendar's square, not a tile's disc.
    cell: bool,
}

impl Mark {
    /// What it is kept under, once per palette.
    fn key(self, dark: bool) -> u32 {
        let color = self.color.map_or(0, |c| c.index() as u32 + 1);
        color
            | u32::from(self.forecast) << 2
            | u32::from(self.unsure) << 3
            | u32::from(self.cell) << 4
            | u32::from(dark) << 5
    }

    /// Swatch.svelte in a 48 box, the web's 2 px ring drawn inside: blue filled, white a ring,
    /// red filled and hatched as `--tempo-red-hatch` says, unknown a dashed muted ring; a
    /// forecast dashed over a wash of its color, its red still hatched.
    fn svg(self, pal: &Palette) -> String {
        let hex = |c: Rgba| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
        let alpha = |c: Rgba| f32::from(c.3) / 255.0;
        let ring = match self.color {
            Some(Color::Blue) => pal.tempo_blue,
            Some(Color::White) => pal.tempo_white_ring,
            Some(Color::Red) => pal.tempo_red,
            None => pal.ink_muted,
        };
        let fill = match (self.color, self.forecast, self.unsure) {
            (Some(Color::Blue), false, _) => format!(r#"fill="{}""#, hex(pal.tempo_blue)),
            (Some(Color::Blue), true, false) => format!(
                r#"fill="{}" fill-opacity="{FORECAST_WASH}""#,
                hex(pal.tempo_blue)
            ),
            (Some(Color::White), false, _) => format!(
                r#"fill="{}" fill-opacity="{}""#,
                hex(pal.tempo_white),
                alpha(pal.tempo_white)
            ),
            (Some(Color::Red), _, _) => r#"fill="url(#h)""#.to_string(),
            _ => r#"fill="none""#.to_string(),
        };
        // `color-mix(in srgb, first p%, towards)`
        let mix = |a: Rgba, b: Rgba, t: f32| {
            let m = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
            Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), 255)
        };
        let first = hatch::first(pal);
        let second = mix(first, hatch::towards(pal), hatch::MIX);
        let dash = if self.unsure {
            r#" stroke-dasharray="1 3""#
        } else if self.color.is_none() || self.forecast {
            r#" stroke-dasharray="4 3""#
        } else {
            ""
        };
        // a forecast's hatching: thin red over nothing
        let (first_o, second_o) = if self.forecast {
            (FORECAST_HATCH, 0.0)
        } else {
            (1.0, 1.0)
        };
        let inset = SWATCH_RING / 2.0;
        let side = 48.0 - SWATCH_RING;
        let shape = if self.cell {
            let r = px::RADIUS_M;
            format!(r#"<rect x="{inset}" y="{inset}" width="{side}" height="{side}" rx="{r}""#)
        } else {
            let r = side / 2.0;
            format!(r#"<circle cx="24" cy="24" r="{r}""#)
        };
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><defs><pattern id="h" width="{period}" height="{period}" patternUnits="userSpaceOnUse" patternTransform="rotate({angle})"><rect width="{stripe}" height="{period}" fill="{first}" fill-opacity="{first_o}"/><rect x="{stripe}" width="{rest}" height="{period}" fill="{second}" fill-opacity="{second_o}"/></pattern></defs>{shape} {fill} stroke="{ring}" stroke-width="{SWATCH_RING}"{dash}/></svg>"##,
            period = hatch::PERIOD,
            angle = hatch::ANGLE,
            stripe = hatch::STRIPE,
            rest = hatch::PERIOD - hatch::STRIPE,
            first = hex(first),
            second = hex(second),
            ring = hex(ring),
        )
    }
}

/// Swatch.svelte's forecast: `color-mix(…18%, transparent)`, its red at 35 %.
const FORECAST_WASH: f32 = 0.18;
const FORECAST_HATCH: f32 = 0.35;

/// A table's first column, of its width.
const TABLE_FIRST: f32 = 0.22;

/// A button's icon and the gap after it (none before no words).
fn icon_room(icon: bool, label: &str) -> (f32, f32) {
    match (icon, label.is_empty()) {
        (false, _) => (0.0, 0.0),
        (true, true) => (ICON, 0.0),
        (true, false) => (ICON, px::S_2),
    }
}

impl P<'_, '_, '_, '_> {
    /// A button's width for its content.
    pub fn button_width(&mut self, b: &Button) -> Result<f32, Error> {
        let (icon, gap) = icon_room(b.icon.is_some() || b.dot.is_some(), &b.label);
        Ok(px::S_3 * 2.0 + icon + gap + self.width(&b.label, ty::LABEL)?)
    }

    pub fn busy(&self, b: &Button) -> bool {
        b.key()
            .is_some_and(|k| matches!(self.c.commands.state(k), State::Pending { .. }))
    }

    /// A button in (x, y, w, h); its words dropped when they do not fit (`.btn-row`).
    pub fn button(&mut self, b: &Button, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let busy = self.busy(b);
        let off = b.unavailable || !self.can(&b.act);
        let fade = |c: Rgba| {
            if off {
                c.faded(px::DISABLED_OPACITY)
            } else {
                c
            }
        };
        let (fill, line, ink) = if b.primary {
            (pal.accent, pal.accent, pal.on_accent)
        } else {
            (pal.surface, pal.line, pal.ink)
        };
        self.framed((x, y, w, h), px::RADIUS, fade(fill), fade(line))?;
        let icon = if busy {
            Some(icon::LOADER_CIRCLE)
        } else {
            b.icon
        };
        let label_w = self.width(&b.label, ty::LABEL)?;
        let (icon_w, gap) = icon_room(icon.is_some() || b.dot.is_some(), &b.label);
        let words =
            !b.label.is_empty() && icon_w + gap + label_w <= w - px::S_3 * 2.0 || icon.is_none();
        let content = icon_w
            + if words {
                gap + label_w.min(w - px::S_3 * 2.0 - icon_w - gap)
            } else {
                0.0
            };
        let mut cx = x + (w - content) / 2.0;
        if let Some(icon) = icon {
            self.icon(icon, cx, y + (h - ICON) / 2.0, ICON, fade(ink))?;
            cx += ICON + gap;
        } else if let Some(dot) = b.dot {
            let ring = Some((RING, fade(pal.line)));
            self.disc(cx, y + (h - ICON) / 2.0, ICON, fade(dot), ring)?;
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
        // busy or out of reach, it keeps its hit: nothing under it is reached instead
        if off || busy {
            self.cover(x, y, w, h);
        } else {
            self.hit(x, y, w, h, Target::Act(b.act.clone()));
        }
        Ok(())
    }

    /// Buttons left to right, wrapping (`.scenes`, `.actions`); the height used.
    pub fn flow(&mut self, buttons: &[Button], x: f32, y: f32, w: f32) -> Result<f32, Error> {
        let (mut cx, mut cy) = (x, y);
        for b in buttons {
            let bw = self.button_width(b)?.min(w);
            if cx > x && cx + bw > x + w {
                cx = x;
                cy += TOUCH + px::S_2;
            }
            self.button(b, cx, cy, bw, TOUCH)?;
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

    pub fn slider(&mut self, s: &Slider, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let value = self.c.sliders.shown(s, self.c.now);
        let off = s.off || self.c.over.gone;
        let fade = |c: Rgba| {
            if off {
                c.faded(px::DISABLED_OPACITY)
            } else {
                c
            }
        };
        let share =
            |v: f64| ((v - s.min) / (s.max - s.min).max(f64::EPSILON)).clamp(0.0, 1.0) as f32;
        // the thumb's centre travels between half a thumb from each end
        let (x0, x1) = (x + px::THUMB / 2.0, x + w - px::THUMB / 2.0);
        let ty0 = y + (px::CONTROL_H - px::TRACK_H) / 2.0;
        self.rounded(x, ty0, w, px::TRACK_H, px::RADIUS_PILL, pal.ground_raised)?;
        let at = x0 + (x1 - x0) * share(value);
        self.rounded(
            x,
            ty0,
            at - x,
            px::TRACK_H,
            px::RADIUS_PILL,
            fade(pal.accent),
        )?;
        // where the device is while it travels, once it is apart from the thumb (Range's mark)
        if let Some(m) = s.mark.filter(|m| (m - value).abs() > s.near) {
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
            Some((THUMB_RING, fade(pal.accent))),
        )?;
        if let Some((left, right)) = &s.ends {
            let ey = y + px::CONTROL_H + px::S_2;
            self.line(left, ty::META, pal.ink_muted, x, ey, w / 2.0)?;
            let rw = self.width(right, ty::META)?;
            self.line(right, ty::META, pal.ink_muted, x + w - rw, ey, w / 2.0)?;
        }
        let what = if off {
            Target::Cover
        } else {
            Target::Slider {
                slider: s.clone(),
                x0: x0 * self.s(),
                x1: x1 * self.s(),
            }
        };
        self.hit(x, y, w, px::CONTROL_H, what);
        Ok(())
    }

    /// A remote's four arrows around its OK, the keys under them, centred (Pad.svelte).
    fn pad(&mut self, p: &Pad, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let (k, ok, g) = (px::REMOTE_KEY, px::REMOTE_OK, px::S_2);
        let (ky, kx) = (y + k + g + (ok - k) / 2.0, x + (w - PAD_W) / 2.0);
        let mid = kx + k + g + (ok - k) / 2.0;
        self.button(&p.up, mid, y, k, k)?;
        self.button(&p.left, kx, ky, k, k)?;
        self.button(&p.ok, kx + k + g, y + k + g, ok, ok)?;
        self.button(&p.right, kx + k + g + ok + g, ky, k, k)?;
        self.button(&p.down, mid, y + k + g + ok + g, k, k)?;
        self.row(&p.under, x, y + PAD_H + px::S_3, w)
    }

    /// Equal buttons side by side.
    fn row(&mut self, buttons: &[Button], x: f32, y: f32, w: f32) -> Result<(), Error> {
        let n = buttons.len().max(1) as f32;
        let bw = (w - px::S_2 * (n - 1.0)) / n;
        for (i, b) in buttons.iter().enumerate() {
            self.button(b, x + i as f32 * (bw + px::S_2), y, bw, TOUCH)?;
        }
        Ok(())
    }

    /// Progress.svelte: its words, the elapsed seconds at the end, a bar.
    fn progress(
        &mut self,
        (label, elapsed): (&str, &str),
        (gone, limit): (Duration, Duration),
        x: f32,
        y: f32,
        w: f32,
    ) -> Result<(), Error> {
        let pal = self.c.pal;
        let t = ty::SECONDARY;
        let seconds = gone.as_secs();
        let said = self.c.words.t(
            elapsed,
            &[("seconds", crate::i18n::Arg::Count(seconds as f64))],
        );
        let sw = self.width(&said, t)?;
        self.line(label, t, pal.ink_muted, x, y, w - sw - px::S_2)?;
        self.line(&said, t, pal.ink_muted, x + w - sw, y, sw)?;
        let (by, h) = (y + t.line + px::S_1, px::BAR_THIN);
        self.rounded(x, by, w, h, h / 2.0, pal.ground_raised)?;
        let share = (gone.as_secs_f32() / limit.as_secs_f32().max(1.0)).min(1.0);
        self.rounded(x, by, w * share, h, h / 2.0, pal.accent)
    }

    /// A cell's side in a `w` wide month.
    fn cell(w: f32) -> f32 {
        ((w - 6.0 * CELL_GAP) / 7.0).min(px::TILE_ICON)
    }

    fn month_height(m: &Month, w: f32) -> f32 {
        let rows = (m.lead + m.days.len()).div_ceil(7) as f32;
        ty::LABEL.line + px::S_2 + ty::META.line + px::S_1 + rows * (Self::cell(w) + CELL_GAP)
    }

    /// TempoCalendar's table: the month's name, the days of the week, its days.
    fn month(&mut self, m: &Month, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let side = Self::cell(w);
        let step = side + CELL_GAP;
        self.line(&m.title, ty::LABEL, pal.ink, x, y, w)?;
        let head = y + ty::LABEL.line + px::S_2;
        for (i, day) in m.weekdays.iter().enumerate() {
            let dw = self.width(day, ty::META)?;
            let dx = x + i as f32 * step + (side - dw) / 2.0;
            self.line(day, ty::META, pal.ink_muted, dx, head, side)?;
        }
        let top = head + ty::META.line + px::S_1;
        for (i, d) in m.days.iter().enumerate() {
            let at = m.lead + i;
            let cx = x + (at % 7) as f32 * step;
            let cy = top + (at / 7) as f32 * step;
            if self.visible(cy, side) {
                self.day(d, cx, cy, side)?;
            }
        }
        Ok(())
    }

    /// A day: its mark, its number, a forecast's probability under it; today ringed.
    fn day(&mut self, d: &Day, x: f32, y: f32, side: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        if d.today {
            let r = px::RADIUS_M;
            let g = px::S_HALF;
            self.rounded(
                x - g,
                y - g,
                side + 2.0 * g,
                side + 2.0 * g,
                r + g,
                pal.accent,
            )?;
            self.rounded(x, y, side, side, r, pal.surface)?;
        }
        let forecast = d.forecast.is_some();
        let unsure = d.forecast.as_ref().is_some_and(|(_, u)| *u);
        self.swatch(
            Mark {
                color: d.color,
                forecast,
                unsure,
                cell: true,
            },
            x,
            y,
            side,
        )?;
        let filled = !forecast && matches!(d.color, Some(Color::Blue | Color::Red));
        let ink = if filled { pal.on_tempo } else { pal.ink };
        let number = d.day.to_string();
        let text_h = ty::META.line + if forecast { ty::TINY.line } else { 0.0 };
        let top = y + (side - text_h) / 2.0;
        let nw = self.width(&number, ty::META)?;
        self.line(&number, ty::META, ink, x + (side - nw) / 2.0, top, side)?;
        if let Some((pct, _)) = &d.forecast {
            let pw = self.width(pct, ty::TINY)?;
            let py = top + ty::META.line;
            self.line(pct, ty::TINY, ink, x + (side - pw) / 2.0, py, side)?;
        }
        Ok(())
    }

    /// A table: the head muted, the first column short and left, the others right, equal.
    fn table(
        &mut self,
        head: &[String],
        rows: &[Vec<String>],
        x: f32,
        y: f32,
        w: f32,
    ) -> Result<(), Error> {
        let pal = self.c.pal;
        let first = w * TABLE_FIRST;
        let cw = (w - first) / head.len().saturating_sub(1).max(1) as f32;
        let mut cy = y;
        let lines = std::iter::once((head, ty::META, pal.ink_muted))
            .chain(rows.iter().map(|r| (r.as_slice(), ty::SECONDARY, pal.ink)));
        for (row, t, ink) in lines {
            for (i, text) in row.iter().enumerate() {
                let tw = self.width(text, t)?.min(cw);
                let tx = match i {
                    0 => x,
                    _ => x + first + i as f32 * cw - tw,
                };
                self.line(text, t, ink, tx, cy, if i == 0 { first } else { cw })?;
            }
            cy += FACT_H;
        }
        Ok(())
    }

    /// Label and value, a line each: the value at the end (StatusRow).
    fn facts(&mut self, facts: &[(String, String)], x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let t = ty::SECONDARY;
        for (i, (label, value)) in facts.iter().enumerate() {
            let fy = y + i as f32 * FACT_H;
            let vw = self.width(value, t)?.min(w * 0.6);
            self.line(label, t, pal.ink_muted, x, fy, w - vw - px::S_2)?;
            self.line(value, t, pal.ink, x + w - vw, fy, vw)?;
        }
        Ok(())
    }

    // ── a tile ──

    fn part_height(&mut self, part: &Part, w: f32) -> Result<f32, Error> {
        Ok(match part {
            Part::Slider(s) => Self::slider_height(s),
            Part::Row(_) => TOUCH,
            Part::Hint(t) => self.para_height(t, ty::SECONDARY, w)?,
            Part::Bar { thin: false, .. } => px::BAR_H,
            Part::Bar { thin: true, .. } => px::BAR_THIN,
            Part::Note { .. } => TOUCH,
            Part::Flow(buttons) => {
                let measuring = std::mem::replace(&mut self.measuring, true);
                let h = self.flow(buttons, 0.0, 0.0, w);
                self.measuring = measuring;
                h?
            }
            Part::Pad(p) if p.under.is_empty() => PAD_H,
            Part::Pad(_) => PAD_H + px::S_3 + TOUCH,
            Part::Facts(facts) => facts.len() as f32 * FACT_H,
            Part::Progress { key, .. } => match self.c.commands.elapsed(key) {
                Some(_) => ty::SECONDARY.line + px::S_1 + px::BAR_THIN,
                None => 0.0,
            },
            Part::Calendar(m) => Self::month_height(m, w),
            Part::Table { rows, .. } => (rows.len() + 1) as f32 * FACT_H,
        })
    }

    /// The head of a tile `w` wide: the glyph's height, or the name (wrapping, `.tile-title`) and
    /// its state line.
    fn head_box(&mut self, t: &Tile, w: f32) -> Result<HeadBox, Error> {
        let inner = w - 2.0 * ROW_PAD_X;
        let fact_w = match &t.fact {
            // at most two fifths of the row: « 47 utilisés sur 300 » whole
            Some(f) => self.width(f, ty::META)?.min(inner * 0.4),
            None => 0.0,
        };
        let name_w =
            inner - px::TILE_ICON - px::S_3 - if fact_w > 0.0 { fact_w + px::S_3 } else { 0.0 };
        let name_h = self.para_height(&t.name, TITLE, name_w)?;
        Ok(HeadBox {
            fact_w,
            name_w,
            name_h,
            h: px::TILE_ICON.max(name_h + ty::SECONDARY.line),
        })
    }

    /// Parts framed without a head: their height whole.
    pub fn parts_height(&mut self, parts: &[Part], w: f32) -> Result<f32, Error> {
        let inner = w - 2.0 * ROW_PAD_X;
        let mut h = ROW_PAD_BOTTOM;
        for p in parts {
            let ph = self.part_height(p, inner)?;
            if ph > 0.0 {
                h += ph + ROW_PAD_BOTTOM;
            }
        }
        Ok(h)
    }

    /// Parts one under the other at (x, y), in a frame `w` wide already drawn.
    pub fn parts(&mut self, parts: &[Part], x: f32, y: f32, w: f32) -> Result<(), Error> {
        let inner = w - 2.0 * ROW_PAD_X;
        let mut py = y + ROW_PAD_BOTTOM;
        for p in parts {
            let ph = self.part_height(p, inner)?;
            if ph > 0.0 {
                self.part(p, x + ROW_PAD_X, py, inner)?;
                py += ph + ROW_PAD_BOTTOM;
            }
        }
        Ok(())
    }

    /// A row's height (the dashboard's `.tile`): its head, each control under it, its padding.
    pub fn tile_height(&mut self, t: &Tile, w: f32) -> Result<f32, Error> {
        let inner = w - 2.0 * ROW_PAD_X;
        let mut h = ROW_PAD_Y + self.head_box(t, w)?.h;
        let mut parts = false;
        for p in &t.parts {
            let ph = self.part_height(p, inner)?;
            if ph > 0.0 {
                h += PART_GAP + ph;
                parts = true;
            }
        }
        Ok(h + if parts { ROW_PAD_BOTTOM } else { ROW_PAD_Y })
    }

    /// The tile's state line, as its gesture says it (DeviceTile's `line`): « Pas de réponse »
    /// once late (and « Réessayer »), « Allumage… » when slow, else the device's words.
    pub fn state_line(&self, t: &Tile) -> (String, bool, Option<Order>) {
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

    /// The order a tap on a toggle sends: to the other side of what it shows (the target while
    /// one travels); none for a device out of reach, or a lead that is no gesture.
    fn toggle(&self, lead: &Lead) -> Option<Order> {
        if self.c.over.gone {
            return None;
        }
        let Lead::Toggle {
            on,
            offline: false,
            turn_on,
            turn_off,
            ..
        } = lead
        else {
            return None;
        };
        let shown = self.c.commands.shown(&turn_on.key, *on);
        Some(Order::clone(if shown { turn_off } else { turn_on }))
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
                turn_off: _,
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
                let fade = |c: Rgba| {
                    if self.c.over.gone {
                        c.faded(px::DISABLED_OPACITY)
                    } else {
                        c
                    }
                };
                self.disc(x, y, size, fade(fill), Some((RING, fade(ring))))?;
                self.icon(*icon, x + inset, y + inset, GLYPH_ICON, fade(ink))?;
                // out of reach, it keeps its hit: the row's page is not opened by a miss
                let what = self
                    .toggle(lead)
                    .map_or(Target::Cover, |o| Target::Act(Act::Order(o)));
                self.hit(x, y, size, size, what);
            }
            Lead::Glyph { icon, on } => {
                let (fill, ink) = if *on {
                    (pal.accent_wash, pal.accent)
                } else {
                    (pal.ground_raised, pal.ink_muted)
                };
                self.rounded(x, y, size, size, px::RADIUS_M, fill)?;
                self.icon(*icon, x + inset, y + inset, GLYPH_ICON, ink)?;
            }
            Lead::Swatch(color) => {
                let mark = Mark {
                    color: *color,
                    forecast: false,
                    unsure: false,
                    cell: false,
                };
                self.swatch(mark, x, y, size)?;
            }
        }
        Ok(())
    }

    /// A Tempo mark `size` square at (x, y), rasterised once.
    fn swatch(&mut self, mark: Mark, x: f32, y: f32, size: f32) -> Result<(), Error> {
        if !self.visible(y, size) {
            return Ok(());
        }
        let pal = self.c.pal;
        let key = mark.key(std::ptr::eq(pal, &DARK));
        let (at, side) = ((self.d(x), self.y(y)), self.d(size).max(1) as u32);
        self.d.mark(key, side, || mark.svg(pal), at)
    }

    /// Shares in Tempo's colours on one track (ForecastCard's `.bar`, Share.svelte's): blue,
    /// white ringed, red, each its share of the width, the rest the empty track.
    fn bar(
        &mut self,
        shares: &[(Color, f64)],
        thin: bool,
        x: f32,
        y: f32,
        w: f32,
    ) -> Result<(), Error> {
        let pal = self.c.pal;
        let h = if thin { px::BAR_THIN } else { px::BAR_H };
        self.rounded(x, y, w, h, h / 2.0, pal.ground_raised)?;
        let mut at = x;
        for (c, share) in shares {
            let sw = w * share.clamp(0.0, 1.0) as f32;
            if sw <= 0.0 {
                continue;
            }
            let fill = match c {
                Color::Blue => pal.tempo_blue,
                Color::White => pal.tempo_white_ring,
                Color::Red => pal.tempo_red,
            };
            self.rounded(at, y, sw, h, if thin { h / 2.0 } else { 0.0 }, fill)?;
            at += sw;
        }
        Ok(())
    }

    /// One row of a group's list at (x, y), `w` wide and `h` high (its frame is the list's).
    pub fn tile(&mut self, t: &Tile, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
        if let Some(open) = &t.open {
            // the whole row leads there (DeviceTile's link); a gesture drawn over it wins
            self.hit(x, y, w, h, Target::Act(open.clone()));
        }
        let head = self.head_box(t, w)?;
        if t.open.is_none()
            && let Some(order) = self.toggle(&t.lead)
        {
            // a row that leads nowhere: its head is its on/off, a big target for a finger on
            // the wall (the controls under it keep theirs); with a page, the disc alone
            self.hit(x, y, w, ROW_PAD_Y + head.h, Target::Act(Act::Order(order)));
        }
        let (ix, iy) = (x + ROW_PAD_X, y + ROW_PAD_Y);
        let inner = w - 2.0 * ROW_PAD_X;
        self.tile_head(t, &head, ix, iy, inner)?;
        let mut py = iy + head.h;
        for p in &t.parts {
            let ph = self.part_height(p, inner)?;
            if ph <= 0.0 {
                continue;
            }
            py += PART_GAP;
            if py > y + h {
                break;
            }
            self.part(p, ix, py, inner)?;
            py += ph;
        }
        Ok(())
    }

    /// The tile's head at (ix, iy), `inner` wide: the lead, the name and the state line (with
    /// « Réessayer » once late), the fact.
    fn tile_head(
        &mut self,
        t: &Tile,
        head: &HeadBox,
        ix: f32,
        iy: f32,
        inner: f32,
    ) -> Result<(), Error> {
        let pal = self.c.pal;
        let tw = head.name_w;
        // the head's row centres its items (`.tile-head`: align-items center)
        self.lead(&t.lead, ix, iy + (head.h - px::TILE_ICON) / 2.0)?;
        let tx = ix + px::TILE_ICON + px::S_3;
        let text_h = head.name_h + ty::SECONDARY.line;
        let ty0 = iy + (head.h - text_h) / 2.0;
        self.para(&t.name, TITLE, pal.ink, tx, ty0, tw)?;
        let (state, warn, retry) = self.state_line(t);
        let sy = ty0 + head.name_h;
        let color = if warn { pal.warn_text } else { pal.ink_muted };
        match retry {
            Some(order) => {
                // a 44 px target around the word
                let pad = (TOUCH - ty::SECONDARY.line) / 2.0;
                let label = self.c.words.s(k::COMMON_RETRY);
                self.link_line(
                    (&state, color),
                    (&label, pal.accent),
                    tx,
                    sy,
                    tw,
                    sy - pad,
                    Some(Act::Order(order)),
                )?;
            }
            None => {
                self.line(&state, ty::SECONDARY, color, tx, sy, tw)?;
            }
        }
        if let Some(f) = &t.fact {
            let fx = ix + inner - head.fact_w;
            self.line(
                f,
                ty::META,
                pal.ink_muted,
                fx,
                iy + (head.h - ty::META.line) / 2.0,
                head.fact_w,
            )?;
        }
        Ok(())
    }

    /// One part under a tile's head at (ix, py), `inner` wide.
    fn part(&mut self, p: &Part, ix: f32, py: f32, inner: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        match p {
            Part::Slider(s) => self.slider(s, ix, py, inner)?,
            Part::Row(buttons) => self.row(buttons, ix, py, inner)?,
            Part::Flow(buttons) => {
                self.flow(buttons, ix, py, inner)?;
            }
            Part::Pad(p) => self.pad(p, ix, py, inner)?,
            Part::Facts(facts) => self.facts(facts, ix, py, inner)?,
            Part::Calendar(m) => self.month(m, ix, py, inner)?,
            Part::Table { head, rows } => self.table(head, rows, ix, py, inner)?,
            Part::Progress {
                key,
                label,
                elapsed,
                limit,
            } => {
                if let Some(gone) = self.c.commands.elapsed(key) {
                    self.progress((label, elapsed), (gone, *limit), ix, py, inner)?;
                }
            }
            Part::Hint(text) => {
                self.para(text, ty::SECONDARY, pal.ink_muted, ix, py, inner)?;
            }
            Part::Bar { shares, thin } => self.bar(shares, *thin, ix, py, inner)?,
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
                let busy = self.busy(link);
                let ink = if busy { pal.ink_muted } else { pal.accent };
                self.link_line(
                    (text, color),
                    (&link.label, ink),
                    ix,
                    ly,
                    inner,
                    py,
                    (!busy).then(|| link.act.clone()),
                )?;
            }
        }
        Ok(())
    }
}
