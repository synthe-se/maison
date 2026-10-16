//! The web app's measures (tokens.css, app.css: the page margin, a group's head, the tiles'
//! grid, a tile's padding and lines), CSS pixels times the scale, and the painter's primitives:
//! boxes, discs, icons, lines and paragraphs at CSS positions, and where each control lands
//! (`hits`), for the finger. `bricks` draws the web app's bricks with them, `screens` the
//! screens.

use std::time::Instant;

use crate::commands::Commands;
use crate::i18n::Words;
use crate::theme::{self, Palette, Rgba, Type, coarse, px, ty, wide};
use crate::ui::gfx::{Draw, Error, Pen};
use crate::ui::icon::Icon;
use crate::ui::model::{Act, Slider};
use crate::ui::slider::Sliders;

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

impl Target {
    /// The key of what it does: an order's, a slider's (a simulation finds controls by it).
    pub fn key(&self) -> Option<&str> {
        match self {
            Self::Act(Act::Order(o)) => Some(&o.key),
            Self::Slider { slider, .. } => Some(&slider.key),
            _ => None,
        }
    }
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

pub const GROUP_HEAD: f32 = px::CONTROL_H;
/// A row's compact controls (`--control-h-s`), grown for a finger (tokens.css `pointer: coarse`).
pub const TOUCH: f32 = coarse::CONTROL_H_S;
/// A tile's name (`.tile-title`: the body, bold).
pub const TITLE: Type = ty::BODY.weight(600);

// The components' own measures, written in their Svelte (not tokens yet): said here once.
/// Icon.svelte's default size (a button's icon).
pub const ICON: f32 = 18.0;
/// DeviceTile's gesture icon.
pub const GLYPH_ICON: f32 = 22.0;
/// A tile's column at least this wide (app.css `.tiles`: `minmax(min(20rem, 100%), 1fr)`).
pub const TILE_MIN: f32 = 320.0;
/// The ring of a gesture's disc (DeviceTile `.gesture`: 1.5 px).
pub const RING: f32 = 1.5;
/// The ring of a slider's thumb (Range `.thumb`: 2 px).
pub const THUMB_RING: f32 = 2.0;
/// A swatch's ring (Swatch.svelte: 2 px).
pub const SWATCH_RING: f32 = 2.0;
/// Stale.svelte's clock.
pub const STALE_ICON: f32 = 14.0;
/// The pairing code's letters apart (ScreenPairing `.code`: 0.15em).
pub const CODE_SPACING: f32 = 0.15;
/// The door's mark (AuthShell `.mark`: 88 px, the name `--s-2` below it).
pub const MARK: f32 = 88.0;
/// Maison's mark, the web app's own (`web/static/brand.svg`), drawn in its colors.
pub const MARK_SVG: &str = include_str!("../../../../web/static/brand.svg");

/// The page margin for the panel's width in CSS pixels (tokens.css `--page-margin` and its
/// media queries).
pub fn margin(width_css: f32) -> f32 {
    theme::at_width(px::PAGE_MARGIN, wide::PAGE_MARGIN, width_css)
}

/// How many tiles side by side in `inner` CSS pixels (`repeat(auto-fill, minmax(20rem, 1fr))`).
pub fn columns(inner: f32) -> usize {
    (((inner + px::S_5) / (TILE_MIN + px::S_5)).floor() as usize).max(1)
}

/// The painter: a drawing's canvas, what it reads, the hits it leaves.
pub struct P<'d, 'a, 't, 'c> {
    pub d: &'d mut Draw<'a, 't>,
    pub c: &'c Ctx<'c>,
    pub hits: Vec<Hit>,
    /// Added to every y: the column's scroll (negative), or 0 for what stays put.
    pub dy: f32,
}

impl P<'_, '_, '_, '_> {
    pub fn s(&self) -> f32 {
        self.c.scale
    }

    /// CSS px → the panel's, rounded (crisp edges).
    pub fn d(&self, v: f32) -> i32 {
        (v * self.s()).round() as i32
    }

    pub fn y(&self, v: f32) -> i32 {
        (v * self.s() + self.dy).round() as i32
    }

    /// Whether (y, h) in CSS px is on the panel at all.
    pub fn visible(&self, y: f32, h: f32) -> bool {
        let top = y * self.s() + self.dy;
        top + h * self.s() >= 0.0 && top <= self.c.height
    }

    pub fn size(&self, t: Type) -> u16 {
        (t.size * self.s()).round().max(1.0) as u16
    }

    pub fn rounded(
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
    pub fn framed(
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
    pub fn disc(
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

    pub fn icon(
        &mut self,
        icon: Icon,
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
        self.d.icon(icon, x, y, px, color)
    }

    /// One line of `t` in a line box at (x, y), cut to `max` CSS px; its width in CSS px.
    pub fn line(
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
    pub fn width(&mut self, text: &str, t: Type) -> Result<f32, Error> {
        let size = self.size(t);
        Ok(self.d.gfx.measure(t.style(), size, 0, text)?.0 as f32 / self.s())
    }

    /// Words wrapped in `w` CSS px; their height in CSS px (whole lines).
    pub fn para_height(&mut self, text: &str, t: Type, w: f32) -> Result<f32, Error> {
        let size = self.size(t);
        let (_, h) = self
            .d
            .gfx
            .measure(t.style(), size, (w * self.s()) as i32, text)?;
        let font_line = self.d.gfx.measure(t.style(), size, 0, "Hg")?.1.max(1);
        let lines = (h as f32 / font_line as f32).round().max(1.0);
        Ok(lines * t.line)
    }

    pub fn para(
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

    pub fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, what: Target) {
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

    /// Words, then « · » and a link (underlined, `ink`), on one secondary line at (x, y) in `w`
    /// CSS px (Stale's « Dernière lecture 14:02 · Réessayer », a tile's « Pas de réponse ·
    /// Réessayer », a shutter's « Fermeture 20:24 · Ne pas fermer ce soir »). The link is a 44 px
    /// target from `target_y` when it acts (`act`).
    #[allow(clippy::too_many_arguments)] // one line's pieces: words, link, where, how
    pub fn link_line(
        &mut self,
        (said, said_color): (&str, Rgba),
        (link, ink): (&str, Rgba),
        x: f32,
        y: f32,
        w: f32,
        target_y: f32,
        act: Option<Act>,
    ) -> Result<(), Error> {
        let t = ty::SECONDARY;
        let used = self.line(said, t, said_color, x, y, w)?;
        let sep_w = self.line(" · ", t, said_color, x + used, y, w - used)?;
        let lx = x + used + sep_w;
        let lw = self.line(link, t, ink, lx, y, w - used - sep_w)?;
        // app.css: `text-underline-offset: var(--s-half)`
        self.rounded(lx, y + t.line - px::S_HALF, lw, 1.0, 0.0, ink)?;
        if let Some(act) = act {
            self.hit(
                lx - px::S_2,
                target_y,
                lw + px::S_4,
                TOUCH,
                Target::Act(act),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_is_the_web_apps() {
        // 800 px wide: 24 px margins, two columns of 20rem and more
        assert_eq!(margin(800.0), 24.0);
        assert_eq!(margin(400.0), 16.0);
        assert_eq!(margin(1600.0), 32.0);
        assert_eq!(columns(752.0), 2);
        assert_eq!(columns(320.0), 1);
        assert_eq!(columns(200.0), 1);
        assert_eq!(columns(1300.0), 3);
    }

    #[test]
    fn a_hit_is_its_box_and_says_its_key() {
        let order = crate::commands::Order::one("k", "x", crate::commands::Post::empty("/x"));
        let h = Hit {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
            what: Target::Act(Act::Order(order)),
        };
        assert!(h.contains(10.0, 20.0) && h.contains(39.9, 59.9));
        assert!(!h.contains(40.0, 30.0) && !h.contains(20.0, 60.0) && !h.contains(9.9, 30.0));
        assert_eq!(h.what.key(), Some("k"));
        assert_eq!(Target::Act(Act::Refresh(&[])).key(), None);
    }
}
