//! The web app's bricks, drawn: a button (`.btn`), a row of them that wraps, a slider (Range),
//! a tile (DeviceTile) with its lead (a gesture, a glyph, a Tempo swatch) and its parts.

use crate::commands::{Order, State};
use crate::i18n::k;
use crate::theme::{Rgba, hatch, px, ty};
use crate::ui::gfx::Error;
use crate::ui::icon;
use crate::ui::layout::{
    GLYPH_ICON, ICON, P, PART_GAP, RING, ROW_PAD_BOTTOM, ROW_PAD_X, ROW_PAD_Y, SWATCH_RING,
    THUMB_RING, TITLE, TOUCH, Target,
};
use crate::ui::model::{Act, Button, Lead, Part, Slider, Tile};
use maison_api::tempo::Color;

impl P<'_, '_, '_, '_> {
    /// A button's width for its content.
    pub fn button_width(&mut self, b: &Button) -> Result<f32, Error> {
        let label = self.width(&b.label, ty::LABEL)?;
        let icon = if b.icon.is_some() {
            ICON + if b.label.is_empty() { 0.0 } else { px::S_2 }
        } else {
            0.0
        };
        Ok(px::S_3 * 2.0 + icon + label)
    }

    pub fn busy(&self, b: &Button) -> bool {
        b.key()
            .is_some_and(|k| matches!(self.c.commands.state(k), State::Pending { .. }))
    }

    /// A button in (x, y, w, h); its words dropped when they do not fit (`.btn-row`).
    pub fn button(&mut self, b: &Button, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
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
        if let Some(icon) = icon {
            self.icon(icon, cx, y + (h - ICON) / 2.0, ICON, fade(ink))?;
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
        let share =
            |v: f64| ((v - s.min) / (s.max - s.min).max(f64::EPSILON)).clamp(0.0, 1.0) as f32;
        // the thumb's centre travels between half a thumb from each end
        let (x0, x1) = (x + px::THUMB / 2.0, x + w - px::THUMB / 2.0);
        let ty0 = y + (px::CONTROL_H - px::TRACK_H) / 2.0;
        self.rounded(x, ty0, w, px::TRACK_H, px::RADIUS_PILL, pal.ground_raised)?;
        let at = x0 + (x1 - x0) * share(value);
        self.rounded(x, ty0, at - x, px::TRACK_H, px::RADIUS_PILL, pal.accent)?;
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
            Some((THUMB_RING, pal.accent)),
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
        let inner = w - 2.0 * ROW_PAD_X;
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

    /// A row's height (the dashboard's `.tile`): its head, each control under it, its padding.
    pub fn tile_height(&mut self, t: &Tile, w: f32) -> Result<f32, Error> {
        let inner = w - 2.0 * ROW_PAD_X;
        let mut h = ROW_PAD_Y + self.head_height(t, w)?;
        for p in &t.parts {
            h += PART_GAP + self.part_height(p, inner)?;
        }
        Ok(h + if t.parts.is_empty() {
            ROW_PAD_Y
        } else {
            ROW_PAD_BOTTOM
        })
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
                self.icon(*icon, x + inset, y + inset, GLYPH_ICON, ink)?;
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
                self.icon(*icon, x + inset, y + inset, GLYPH_ICON, ink)?;
            }
            Lead::Swatch(color) => self.swatch(*color, x, y, size)?,
        }
        Ok(())
    }

    /// Swatch.svelte at its tile size: blue filled, white a ring, red filled and hatched as
    /// `--tempo-red-hatch` says, unknown a dashed muted ring; a 2 px ring in its color.
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
        // `color-mix(in srgb, first p%, towards)`
        let mix = |a: Rgba, b: Rgba, t: f32| {
            let m = |x: u8, y: u8| (f32::from(x) * t + f32::from(y) * (1.0 - t)).round() as u8;
            Rgba(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2), 255)
        };
        let first = hatch::first(pal);
        let second = hex(mix(first, hatch::towards(pal), hatch::MIX));
        let dash = if color.is_none() {
            r#" stroke-dasharray="4 3""#
        } else {
            ""
        };
        let r = 24.0 - SWATCH_RING / 2.0;
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><defs><pattern id="h" width="{period}" height="{period}" patternUnits="userSpaceOnUse" patternTransform="rotate({angle})"><rect width="{stripe}" height="{period}" fill="{first}"/><rect x="{stripe}" width="{rest}" height="{period}" fill="{second}"/></pattern></defs><circle cx="24" cy="24" r="{r}" {fill} stroke="{ring}" stroke-width="{SWATCH_RING}"{dash}/></svg>"##,
            period = hatch::PERIOD,
            angle = hatch::ANGLE,
            stripe = hatch::STRIPE,
            rest = hatch::PERIOD - hatch::STRIPE,
            first = hex(first),
            ring = hex(ring),
        );
        let (px_, py, pw) = (self.d(x), self.y(y), self.d(size).max(1) as u32);
        self.d.picture(&svg, px_, py, pw, pw)
    }

    /// One row of a group's list at (x, y), `w` wide and `h` high (its frame is the list's).
    pub fn tile(&mut self, t: &Tile, x: f32, y: f32, w: f32, h: f32) -> Result<(), Error> {
        let (ix, iy) = (x + ROW_PAD_X, y + ROW_PAD_Y);
        let inner = w - 2.0 * ROW_PAD_X;
        let head_h = self.tile_head(t, ix, iy, w)?;
        let mut py = iy + head_h;
        for p in &t.parts {
            py += PART_GAP;
            let ph = self.part_height(p, inner)?;
            if py > y + h {
                break;
            }
            self.part(p, ix, py, inner)?;
            py += ph;
        }
        Ok(())
    }

    /// The tile's head at (ix, iy) inside a tile `w` wide: the lead, the name and the state
    /// line (with « Réessayer » once late), the fact; its height.
    fn tile_head(&mut self, t: &Tile, ix: f32, iy: f32, w: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        let inner = w - 2.0 * ROW_PAD_X;
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
        Ok(head_h)
    }

    /// One part under a tile's head at (ix, py), `inner` wide.
    fn part(&mut self, p: &Part, ix: f32, py: f32, inner: f32) -> Result<(), Error> {
        let pal = self.c.pal;
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
