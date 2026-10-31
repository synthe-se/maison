//! The screens, drawn from the bricks: the house (the header with the time and the date, then
//! each group: its head, its « old » line, its body), the door (a screen not paired: the brand,
//! words, a code, a button), and over them the toasts, a dialog, what the finger presses, and
//! one band while Maison does not answer (no line per group, nothing moves).

use crate::commands::Toast;
use crate::i18n::k;
use crate::theme::{px, ty};
use crate::ui::gfx::{Draw, Error};
use crate::ui::icon;
use crate::ui::layout::{
    BRAND_NUDGE, CHIP, CHIP_ICON, CLOCK, CODE_SPACING, COLUMN_GAP, Ctx, DIALOG_W, Drawn, GROUP_GAP,
    GROUP_HEAD, HEAD_GAP, HEADER_MARK, HOUSE_HEAD, MARK, MARK_SVG, NOW_LINES, P, STALE_ICON, TOUCH,
    Target, balance, clip_above, columns, fold, margin,
};
use crate::ui::model::{Act, Asked, Body, Button, Chip, Group, Screen};

/// What the header says: the brand (the house), or a page's way back and its title.
enum Head<'a> {
    /// The home's: the time and the date.
    Brand(&'a str, &'a str),
    Back(&'a Button, &'a str),
}

impl Head<'_> {
    fn height(&self) -> f32 {
        match self {
            Self::Brand(..) => HOUSE_HEAD,
            Self::Back(..) => px::HEADER_H,
        }
    }
}

impl P<'_, '_, '_, '_> {
    /// A group at (x, y), `w` wide (Group.svelte); its height.
    fn group(&mut self, g: &Group, x: f32, y: f32, w: f32) -> Result<f32, Error> {
        self.group_head(g, x, y, w)?;
        let mut cy = y + GROUP_HEAD + HEAD_GAP;
        // Maison gone, one band says it for every group
        if let Some((said, retry)) = g.stale.as_ref().filter(|_| !self.c.over.gone) {
            self.stale(said, retry, x, cy, w)?;
            cy += TOUCH + HEAD_GAP;
        }
        cy += self.body(&g.body, x, cy, w)?;
        Ok(cy - y)
    }

    /// The head: the title, then at its end the fact and the actions.
    fn group_head(&mut self, g: &Group, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let mut end = x + w;
        if g.anchor.is_some() && g.anchor == self.c.over.flash {
            self.rounded(x, y, w, GROUP_HEAD, px::RADIUS, pal.accent_wash)?;
        }
        let mut widths = Vec::with_capacity(g.actions.len());
        for b in &g.actions {
            widths.push(self.button_width(b)?);
        }
        if !widths.is_empty() {
            let actions_w = widths.iter().sum::<f32>() + px::S_2 * (widths.len() - 1) as f32;
            let mut bx = end - actions_w;
            for (b, bw) in g.actions.iter().zip(widths) {
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
        Ok(())
    }

    /// « Maintenant » at (x, y), `w` wide: its chips on two lines at most, the rest behind
    /// « +N », which shows them all; each leads to its group; its height.
    fn now_strip(&mut self, chips: &[Chip], x: f32, y: f32, w: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        let h = TOUCH;
        let gap = px::S_1;
        let mut widths = Vec::with_capacity(chips.len());
        for c in chips {
            let icon = if c.warn { CHIP_ICON + px::S_1 } else { 0.0 };
            widths.push(self.width(&c.text, CHIP)? + 2.0 * px::S_3 + icon);
        }
        let mut mores = Vec::new();
        for n in 0..=chips.len() {
            mores.push(self.width(&format!("+{n}"), CHIP)? + 2.0 * px::S_3);
        }
        let warn: Vec<bool> = chips.iter().map(|c| c.warn).collect();
        let lines = if self.c.over.unfolded {
            chips.len()
        } else {
            NOW_LINES
        };
        let (shown, folded) = fold(&widths, &warn, w, gap, lines, |n| mores[n]);
        let more = format!("+{folded}");
        let pills = chips
            .iter()
            .zip(&widths)
            .zip(&shown)
            .filter(|(_, s)| **s)
            .map(|((c, cw), _)| (c.text.as_str(), c.warn, *cw, Act::Jump(c.to)))
            .chain((folded > 0).then(|| (more.as_str(), false, mores[folded], Act::Unfold)));
        let (mut cx, mut cy) = (x, y);
        for (text, warn, cw, act) in pills {
            if cx > x && cx + cw > x + w {
                cx = x;
                cy += h + gap;
            }
            let (fill, line, ink) = if warn {
                (pal.blocked_wash, pal.blocked, pal.warn_text)
            } else {
                (pal.ground, pal.line, pal.ink)
            };
            self.framed((cx, cy, cw, h), px::RADIUS_PILL, fill, line)?;
            self.hit(cx, cy, cw, h, Target::Act(act));
            let mut tx = cx + px::S_3;
            if warn {
                self.icon(
                    icon::TRIANGLE_ALERT,
                    tx,
                    cy + (h - CHIP_ICON) / 2.0,
                    CHIP_ICON,
                    ink,
                )?;
                tx += CHIP_ICON + px::S_1;
            }
            self.line(text, CHIP, ink, tx, cy + (h - CHIP.line) / 2.0, cw)?;
            cx += cw + gap;
        }
        Ok(cy + h - y)
    }

    /// Stale.svelte at (x, cy): the clock, « Dernière lecture 14:02 · Réessayer ».
    fn stale(&mut self, said: &str, retry: &Button, x: f32, cy: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        self.icon(
            icon::CLOCK,
            x,
            cy + (TOUCH - STALE_ICON) / 2.0,
            STALE_ICON,
            pal.ink_muted,
        )?;
        let tx = x + STALE_ICON + px::S_1;
        let ly = cy + (TOUCH - ty::SECONDARY.line) / 2.0;
        self.link_line(
            (said, pal.ink_muted),
            (&retry.label, pal.accent),
            tx,
            ly,
            x + w - tx,
            cy,
            Some(retry.act.clone()),
        )
    }

    /// Loaded's states, then the devices (one framed list of rows) or the buttons; its height.
    fn body(&mut self, body: &Body, x: f32, y: f32, w: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        let mut cy = y;
        // a row's place, held (Loaded's skeleton: a gesture and its padding); Maison gone, the
        // band says why, not each group
        let held = px::TILE_ICON + 2.0 * px::S_1;
        match body {
            Body::Loading => {
                self.rounded(x, cy, w, held, px::RADIUS_L, pal.ground_raised)?;
                cy += held;
            }
            Body::Failed { .. } if self.c.over.gone => {
                self.rounded(x, cy, w, held, px::RADIUS_L, pal.ground_raised)?;
                cy += held;
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
            Body::Buttons(buttons) => cy += self.flow(buttons, x, cy, w)?,
            Body::Parts(parts) => {
                let h = self.parts_height(parts, w)?;
                self.framed((x, cy, w, h), px::RADIUS_L, pal.surface, pal.line)?;
                if self.visible(cy, h) {
                    self.parts(parts, x, cy, w)?;
                }
                cy += h;
            }
            Body::Tiles(tiles) => {
                let mut hs = Vec::with_capacity(tiles.len());
                for t in tiles {
                    hs.push(self.tile_height(t, w)?);
                }
                let total: f32 = hs.iter().sum();
                self.framed((x, cy, w, total), px::RADIUS_L, pal.surface, pal.line)?;
                let hair = 1.0 / self.s();
                for (i, (t, th)) in tiles.iter().zip(&hs).enumerate() {
                    if i > 0 {
                        // a line between two rows (`.tile + .tile`'s inset shadow)
                        self.rounded(x + hair, cy, w - 2.0 * hair, hair, 0.0, pal.line)?;
                    }
                    if self.visible(cy, *th) {
                        self.tile(t, x, cy, w, *th)?;
                    }
                    cy += th;
                }
            }
        }
        Ok(cy - y)
    }

    /// The house, or one of its pages: the groups in columns (the dashboard's), the header
    /// over them; the columns' height in the panel's pixels.
    fn house(
        &mut self,
        now: &[Chip],
        groups: &[Group],
        head: Head,
        wcss: f32,
    ) -> Result<f32, Error> {
        let pal = self.c.pal;
        let m = margin(wcss);
        let inner = wcss - 2.0 * m;
        let cols = columns(inner);
        let cw = (inner - COLUMN_GAP * (cols as f32 - 1.0)) / cols as f32;
        // each group's height, from the code that draws it
        let measuring = std::mem::replace(&mut self.measuring, true);
        let mut hs = Vec::with_capacity(groups.len());
        for g in groups {
            hs.push(self.group(g, 0.0, 0.0, cw)?);
        }
        self.measuring = measuring;
        let starts = balance(&hs, cols, GROUP_GAP);
        let head_h = head.height();
        let mut top = head_h + px::S_5;
        if !now.is_empty() {
            top += self.now_strip(now, m, top, inner)? + px::S_4;
        }
        let mut bottom = top;
        for (c, &from) in starts.iter().enumerate() {
            let to = starts.get(c + 1).copied().unwrap_or(groups.len());
            let x = m + c as f32 * (cw + COLUMN_GAP);
            let mut y = top;
            for (g, h) in groups[from..to].iter().zip(&hs[from..to]) {
                self.group(g, x, y, cw)?;
                if let Some(anchor) = g.anchor {
                    let s = self.s();
                    self.anchors.push((anchor, (y - head_h - GROUP_GAP) * s));
                }
                y += h + GROUP_GAP;
            }
            bottom = bottom.max(y - GROUP_GAP);
        }
        // the band over the bottom hides nothing for good: the column goes on past it
        let band = if self.c.over.gone { TOUCH } else { 0.0 };
        let content = (bottom + px::S_5 + band) * self.s();
        // the header stays, a line under it: the brand, or a page's way back and its title; what
        // scrolled under it is hidden from the finger too
        self.dy = 0.0;
        let edge = head_h * self.s();
        clip_above(&mut self.hits, edge);
        self.cover(0.0, 0.0, wcss, head_h);
        self.rounded(0.0, 0.0, wcss, head_h, 0.0, pal.ground)?;
        let hair = 1.0 / self.s();
        self.rounded(0.0, head_h - hair, wcss, hair, 0.0, pal.line)?;
        let (time, date) = match head {
            Head::Back(back, title) => {
                let bw = self.button_width(back)?;
                self.button(back, m, (head_h - TOUCH) / 2.0, bw, TOUCH)?;
                let tx = m + bw + px::S_4;
                self.line(
                    title,
                    ty::GROUP,
                    pal.ink,
                    tx,
                    (head_h - ty::GROUP.line) / 2.0,
                    wcss - tx - m,
                )?;
                return Ok(content);
            }
            Head::Brand(time, date) => (time, date),
        };
        let my = (head_h - HEADER_MARK) / 2.0;
        if self.visible(my, HEADER_MARK) {
            let mark = self.d(HEADER_MARK).max(1) as u32;
            self.d
                .picture(MARK_SVG, self.d(m), self.y(my), mark, mark)?;
        }
        let bx = m + HEADER_MARK + px::S_2;
        let brand = self.c.words.s(k::BRANDING_NAME);
        let brand_w = self.line(
            &brand,
            ty::BRAND,
            pal.ink,
            bx,
            (head_h - ty::BRAND.line) / 2.0 + BRAND_NUDGE * ty::BRAND.size,
            wcss - bx - m,
        )?;
        // the time at the end, the date before it
        let tw = self.width(time, CLOCK)?;
        let tx = wcss - m - tw;
        self.line(time, CLOCK, pal.ink, tx, (head_h - CLOCK.line) / 2.0, tw)?;
        let room = tx - px::S_4 - (bx + brand_w + px::S_4);
        let dw = self.width(date, ty::BODY)?.min(room);
        let dy = (head_h - ty::BODY.line) / 2.0;
        self.line(date, ty::BODY, pal.ink_muted, tx - px::S_4 - dw, dy, dw)?;
        Ok(content)
    }

    /// The house-wide word while Maison does not answer, along the panel's bottom; its height.
    fn gone_band(&mut self, wcss: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        self.dy = 0.0;
        let y = self.c.height / self.s() - TOUCH;
        self.rounded(0.0, y, wcss, TOUCH, 0.0, pal.blocked_wash)?;
        self.rounded(0.0, y, wcss, 1.0 / self.s(), 0.0, pal.blocked)?;
        let m = margin(wcss);
        let iy = y + (TOUCH - CHIP_ICON) / 2.0;
        self.icon(icon::TRIANGLE_ALERT, m, iy, CHIP_ICON, pal.warn_text)?;
        let said = self.c.words.s(k::SCREEN_WAITING_BODY);
        let tx = m + CHIP_ICON + px::S_2;
        let ly = y + (TOUCH - ty::BODY.line) / 2.0;
        self.line(&said, ty::BODY, pal.warn_text, tx, ly, wcss - tx - m)?;
        self.cover(0.0, y, wcss, TOUCH);
        Ok(TOUCH)
    }

    /// ConfirmDialog, over everything: the consequence said, the precise verb and « Garder »,
    /// no default button; a finger anywhere else reaches nothing.
    fn dialog(&mut self, a: &Asked, wcss: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        self.dy = 0.0;
        let hcss = self.c.height / self.s();
        self.rounded(0.0, 0.0, wcss, hcss, 0.0, pal.overlay)?;
        self.cover(0.0, 0.0, wcss, hcss);
        let w = (wcss - 2.0 * margin(wcss)).min(DIALOG_W);
        let inner = w - 2.0 * px::S_5;
        let title_h = self.para_height(&a.title, ty::GROUP, inner)?;
        let text_h = self.para_height(&a.text, ty::BODY, inner)?;
        let h = 2.0 * px::S_5 + title_h + px::S_3 + text_h + px::S_5 + TOUCH;
        let (x, y) = ((wcss - w) / 2.0, (hcss - h) / 2.0);
        self.framed((x, y, w, h), px::RADIUS_L, pal.surface, pal.line)?;
        let (ix, mut cy) = (x + px::S_5, y + px::S_5);
        cy += self.para(&a.title, ty::GROUP, pal.ink, ix, cy, inner)? + px::S_3;
        cy += self.para(&a.text, ty::BODY, pal.ink_muted, ix, cy, inner)? + px::S_5;
        let keep = Button::new(None, self.c.words.s(k::DEVICE_KEEP), Act::Keep);
        let sure = Button::new(None, a.action.clone(), Act::Sure);
        let bw = (inner - px::S_2) / 2.0;
        self.button(&keep, ix, cy, bw, TOUCH)?;
        self.button(&sure, ix + bw + px::S_2, cy, bw, TOUCH)
    }

    /// What the finger is on: a wash over its box, whatever the target (docs/ux.md § 7).
    fn pressed(&mut self, (x, y, w, h): (f32, f32, f32, f32)) -> Result<(), Error> {
        let s = self.s();
        let r = if w == h { w / 2.0 / s } else { px::RADIUS };
        self.dy = 0.0;
        let wash = self.c.pal.ink.faded(px::FAINT_OPACITY);
        self.rounded(x / s, y / s, w / s, h / s, r, wash)
    }

    #[allow(clippy::too_many_arguments)] // the door's pieces, as `Screen::Door` holds them
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
        let mut h = MARK
            + px::S_2
            + ty::BRAND_DOOR.line
            + px::S_5
            + self.para_height(title, ty::PAGE, w)?;
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
        let mark = self.d(MARK).max(1) as u32;
        self.d.picture(MARK_SVG, self.d(x), self.y(y), mark, mark)?;
        y += MARK + px::S_2;
        self.line(&brand, ty::BRAND_DOOR, pal.ink, x, y, w)?;
        y += ty::BRAND_DOOR.line + px::S_5;
        y += self.para(title, ty::PAGE, pal.ink, x, y, w)?;
        for p in lead {
            y += px::S_4;
            let ink = if warn { pal.warn_text } else { pal.ink };
            y += self.para(p, ty::BODY, ink, x, y, w)?;
        }
        if let Some(code) = code {
            y += px::S_5;
            // read across the room: its characters apart
            let spacing = ty::CODE.size * CODE_SPACING;
            let mut cx = x;
            for ch in code.chars() {
                let cw = self.line(&ch.to_string(), ty::CODE, pal.accent, cx, y, w)?;
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

    /// The toasts, at the bottom, above everything (app.css `.toasts`, `.toast`, `.toast.warn`).
    /// The photos, and the time small in a corner, on what keeps it legible over any picture.
    /// Nothing else: no band, no toast, no target.
    fn photos(&mut self, time: &str, wcss: f32) -> Result<(), Error> {
        let (pal, fade) = (self.c.pal, self.c.fade);
        self.d.photos(fade, self.c.width, self.c.height)?;
        self.dy = 0.0;
        let w = self.width(time, ty::META)?;
        let h = ty::META.line + 2.0 * px::S_1;
        let x = wcss - px::S_4 - w - 2.0 * px::S_2;
        let y = self.c.height / self.s() - px::S_4 - h;
        self.rounded(x, y, w + 2.0 * px::S_2, h, px::RADIUS, pal.overlay)?;
        self.line(time, ty::META, pal.on_brand, x + px::S_2, y + px::S_1, w)?;
        Ok(())
    }

    fn toasts(&mut self, toasts: &[Toast], wcss: f32, band: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        self.dy = 0.0;
        let tw = (wcss - px::S_6).min(px::TOAST_W);
        let x = (wcss - tw) / 2.0;
        let text_w = tw - px::S_4 - px::S_3;
        let mut y = self.c.height / self.s() - px::S_4 - band;
        for t in toasts.iter().rev() {
            let text_h = self.para_height(&t.text, ty::BODY, text_w)?;
            let th = (text_h + 2.0 * px::S_2).max(TOUCH);
            y -= th;
            let ink = if t.warn {
                self.framed((x, y, tw, th), px::RADIUS_L, pal.blocked_wash, pal.blocked)?;
                pal.warn_text
            } else {
                self.rounded(x, y, tw, th, px::RADIUS_L, pal.ink)?;
                pal.ground
            };
            self.para(
                &t.text,
                ty::BODY,
                ink,
                x + px::S_4,
                y + (th - text_h) / 2.0,
                text_w,
            )?;
            self.cover(x, y, tw, th);
            y -= px::S_2;
        }
        Ok(())
    }
}

/// Draws the screen; what a finger can land on, and the column's height.
pub fn paint(d: &mut Draw, c: &Ctx, screen: &Screen, toasts: &[Toast]) -> Result<Drawn, Error> {
    d.clear(c.pal.ground);
    let mut p = P {
        d,
        c,
        hits: Vec::new(),
        dy: -c.scroll,
        measuring: false,
        anchors: Vec::new(),
    };
    let wcss = c.width / c.scale;
    let content = match screen {
        Screen::House {
            time,
            date,
            now,
            groups,
        } => p.house(now, groups, Head::Brand(time, date), wcss)?,
        Screen::Page {
            title,
            back,
            groups,
        } => p.house(&[], groups, Head::Back(back, title), wcss)?,
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
            0.0
        }
        Screen::Photos { time, .. } => {
            p.photos(time, wcss)?;
            return Ok(Drawn {
                hits: Vec::new(),
                content: 0.0,
                anchors: Vec::new(),
            });
        }
    };
    let band = if c.over.gone && !matches!(screen, Screen::Door { .. }) {
        p.gone_band(wcss)?
    } else {
        0.0
    };
    if let Some(a) = c.over.asking {
        p.dialog(a, wcss)?;
    }
    p.toasts(toasts, wcss, band)?;
    if let Some(r) = c.over.pressed {
        p.pressed(r)?;
    }
    Ok(Drawn {
        hits: p.hits,
        content,
        anchors: p.anchors,
    })
}

#[cfg(test)]
mod tests {
    //! Drawn for real, on SDL's dummy video driver and its software renderer: no window shown,
    //! no GPU. SDL is set up by this one test (the thread that sets it up is SDL's main one).

    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::client::Client;
    use crate::commands::{Commands, Order, Post};
    use crate::families::lamps::{HUE_DRIVER, Lamp, lamps_group};
    use crate::i18n::{Lang, Words};
    use crate::live::View;
    use crate::testing::{temp_dir, wait_until};
    use crate::theme::{Face, LIGHT, Rgba, Style};
    use crate::ui::gfx::{Gfx, Pen};
    use crate::ui::layout::{Hit, Over};
    use crate::ui::model::{Lead, Part, Tile};
    use crate::ui::slider::Sliders;
    use maison_api::HOUSE_TZ;

    fn lamp(id: &str, on: bool) -> Lamp {
        Lamp {
            id: id.into(),
            name: format!("Lampe {id}"),
            reachable: true,
            connecting: false,
            last_seen: None,
            is_on: on,
            brightness: 60,
            temperature: None,
            color: false,
        }
    }

    #[test]
    fn a_screen_draws_its_controls_where_a_finger_finds_them() {
        assert!(sdl3::hint::set("SDL_VIDEO_DRIVER", "dummy"));
        let sdl = sdl3::init().unwrap();
        let video = sdl.video().unwrap();
        let window = video.window("test", 800, 1280).build().unwrap();
        let mut canvas = sdl3::render::create_renderer(window, Some(c"software")).unwrap();
        let creator = canvas.texture_creator();
        let mut gfx = Gfx::new(sdl3::ttf::init().unwrap(), &creator).unwrap();
        let words = Words::new(Lang::Fr);

        let style = Style {
            face: Face::Text,
            weight: 400,
        };
        let long = "Une lampe au nom bien trop long pour sa tuile";

        // a size asked of SDL_ttf is the drawn line's, in one line or wrapped, and makes no line
        let one = gfx.measure(style, 16, 0, long).unwrap();
        let wrapped = gfx.measure(style, 16, 120, long).unwrap();
        assert!(wrapped.1 > one.1 && wrapped.0 <= 120, "{wrapped:?}");
        assert_eq!(gfx.measure(style, 16, 0, "").unwrap(), (0, 0));
        assert_eq!(gfx.measure(style, 16, 0, "\0").unwrap(), (0, 0));
        assert!(gfx.lines.is_empty(), "measuring draws nothing");
        for (wrap, size) in [(0, one), (120, wrapped)] {
            let pen = Pen {
                style,
                size: 16,
                color: LIGHT.ink,
                wrap,
            };
            assert_eq!(gfx.line(pen, long).unwrap().size, size);
        }

        // a picture is rasterised per size, not minified from the first one
        for side in [28, 88, 28] {
            gfx.picture(MARK_SVG, side, side).unwrap();
        }
        assert_eq!(gfx.pictures.len(), 2);
        assert_eq!(gfx.pictures[&(88, 88)].len(), 1);

        // fit: cut with an ellipsis within its room; a NUL dropped, never cutting words short
        assert_eq!(gfx.fit(style, 16, long, 0).unwrap(), "…", "no room at all");
        let ellipsis = gfx.measure(style, 16, 0, "…").unwrap().0;
        assert_eq!(gfx.fit(style, 16, long, ellipsis).unwrap(), "…");
        let fitted = gfx.fit(style, 16, long, 120).unwrap();
        assert!(
            fitted.ends_with('…') && fitted.len() < long.len(),
            "{fitted}"
        );
        assert!(gfx.measure(style, 16, 0, &fitted).unwrap().0 <= 120);
        assert_eq!(gfx.fit(style, 16, "ab\0cd", 1000).unwrap(), "abcd");
        let cut = gfx
            .fit(style, 16, "\0\0\0abcdefghijklmnopqrstuvwxyz", 60)
            .unwrap();
        assert!(cut.starts_with("abc") && cut.ends_with('…'), "{cut}");

        // one lamp's gesture went unanswered: late at once (a limit of nothing)
        let client = Client::new(
            "http://127.0.0.1:9",
            &temp_dir("paint"),
            Duration::from_secs(1),
        );
        let mut commands = Commands::new(client, Arc::new(|| {}));
        let now_tz = chrono::Utc::now().with_timezone(&HOUSE_TZ);
        let dark = lamp("2", false);
        let late_tile = crate::families::lamps::tile(&words, HUE_DRIVER, &dark, &now_tz);
        let Lead::Toggle { turn_on, .. } = &late_tile.lead else {
            panic!("a toggle")
        };
        commands.run(Order::clone(turn_on).within(Duration::ZERO));
        let both = View::ready(vec![lamp("1", true), dark.clone()]);
        let group = lamps_group(&words, &[(HUE_DRIVER, &both)], &now_tz, None).unwrap();
        let sliders = Sliders::default();
        let ctx = Ctx {
            pal: &LIGHT,
            words: &words,
            commands: &commands,
            sliders: &sliders,
            now: Instant::now(),
            scale: 1.0,
            width: 800.0,
            height: 1280.0,
            scroll: 0.0,
            over: Over::default(),
            fade: 1.0,
        };
        let toasts = [Toast {
            text: "Pas de réponse".into(),
            warn: true,
        }];
        let mut d = Draw {
            canvas: &mut canvas,
            gfx: &mut gfx,
        };
        let house = Screen::House {
            time: "19:05".into(),
            date: words.long_date(now_tz.date_naive()),
            now: vec![Chip {
                text: "2 lampes allumées".into(),
                warn: false,
                to: "lamps",
            }],
            groups: vec![group.clone()],
        };
        let drawn = paint(&mut d, &ctx, &house, &toasts).unwrap();
        let keys: Vec<String> = drawn
            .hits
            .iter()
            .filter_map(|h| Some(h.what.key()?.into_owned()))
            .collect();
        // « Tout éteindre », each gesture (its row's head and its disc), the lit one's slider,
        // and the late one's « Réessayer », which turns it on again
        let has = |k: &str| keys.iter().any(|x| x == k);
        assert!(has("lamps:all-off"), "{keys:?}");
        assert!(has("jump:lamps"), "a chip leads to its group: {keys:?}");
        assert_eq!(
            drawn.anchors,
            [("lamps", px::S_5 + TOUCH + px::S_4 - GROUP_GAP)],
            "under the header, chips above it"
        );
        assert_eq!(
            keys.iter().filter(|k| *k == "hue-lamp:2:power").count(),
            3,
            "the late lamp's row, its disc and its « Réessayer »: {keys:?}"
        );
        let row = drawn
            .hits
            .iter()
            .find(|h| h.what.key().as_deref() == Some("hue-lamp:1:power"))
            .unwrap();
        assert!(row.w > 300.0, "the whole row is the lamp's on/off: {row:?}");
        assert!(has("hue-lamp:1:power") && has("hue-lamp:1:brightness"));
        let slider = drawn
            .hits
            .iter()
            .find(|h| matches!(h.what, Target::Slider { .. }))
            .unwrap();
        assert!(slider.w > 200.0 && slider.h == px::CONTROL_H);
        // every hit is on the panel, a finger's size at least
        for h in &drawn.hits {
            assert!(h.x >= 0.0 && h.x + h.w <= 800.0 && h.y >= 0.0, "{h:?}");
            assert!(h.h >= TOUCH.min(px::TILE_ICON), "{h:?}");
        }
        assert!(drawn.content > px::HEADER_H);
        // the header and the toast cover what lies under them
        let covers: Vec<&Hit> = drawn
            .hits
            .iter()
            .filter(|h| matches!(h.what, Target::Cover))
            .collect();
        assert_eq!(covers.len(), 2);
        assert!(covers[0].contains(400.0, 10.0));
        assert!(covers[1].contains(400.0, 1280.0 - 2.0 * px::S_4));
        assert!(matches!(drawn.hits.last().unwrap().what, Target::Cover));

        // scrolled: nothing left to press under the header
        let scrolled = Ctx {
            fade: 1.0,
            scroll: 70.0,
            pal: &LIGHT,
            words: &words,
            commands: &commands,
            sliders: &sliders,
            now: ctx.now,
            scale: 1.0,
            width: 800.0,
            height: 1280.0,
            over: Over::default(),
        };
        let moved = paint(&mut d, &scrolled, &house, &[]).unwrap();
        assert!(
            moved
                .hits
                .iter()
                .filter(|h| !matches!(h.what, Target::Cover))
                .all(|h| h.y >= px::HEADER_H),
            "{:?}",
            moved.hits
        );

        // the state lines, as each gesture says it
        {
            let p = P {
                d: &mut d,
                c: &ctx,
                hits: Vec::new(),
                dy: 0.0,
                measuring: false,
                anchors: Vec::new(),
            };
            let Body::Tiles(tiles) = &group.body else {
                panic!("tiles")
            };
            let (said, warn, retry) = p.state_line(&tiles[1]);
            assert_eq!(said, words.s(k::COMMAND_NO_ANSWER_SHORT));
            assert!(warn);
            assert_eq!(retry.map(|o| o.target), Some(Some(true)));
            let (said, warn, retry) = p.state_line(&tiles[0]);
            assert_eq!(said, tiles[0].state);
            assert!(!warn && retry.is_none());
        }

        // a ringed box is the box and its inset one, whatever is drawn or skipped under it
        let pixels = |d: &mut Draw, draw: &dyn Fn(&mut Draw)| {
            d.clear(LIGHT.ground);
            draw(d);
            let s = d.canvas.read_pixels(None).unwrap();
            s.with_lock(<[u8]>::to_vec)
        };
        let area = (40, 40, 200, 60);
        let (x, y, w, h) = area;
        let (ring, opaque, wash) = (LIGHT.line, LIGHT.surface, LIGHT.surface.faded(0.5));
        for (r, t, fill) in [
            (16, 1, opaque),
            (30, 1, opaque),
            (6, 3, opaque),
            (16, 1, wash),
        ] {
            let two = pixels(&mut d, &|d| {
                d.rounded(x, y, w, h, r, ring).unwrap();
                d.rounded(x + t, y + t, w - 2 * t, h - 2 * t, r - t, fill)
                    .unwrap();
            });
            let one = pixels(&mut d, &|d| d.ringed(area, r, t, fill, ring).unwrap());
            assert!(two == one, "radius {r}, ring {t}");
        }
        // a box's pieces leave no seam: its straight sides solid, its corners' edge not
        let pixels_of_box = pixels(&mut d, &|d| {
            d.rounded(0, 0, 100, 40, 12, Rgba(255, 0, 0, 255)).unwrap();
        });
        let stride = pixels_of_box.len() / 1280;
        let at = |x: usize, y: usize| &pixels_of_box[y * stride + x * 4..][..4];
        let red = at(50, 20).to_vec();
        for (px, py) in [
            (12, 0),
            (50, 0),
            (87, 39),
            (0, 12),
            (99, 27),
            (12, 12),
            (87, 27),
        ] {
            assert_eq!(at(px, py), red, "({px}, {py})");
        }
        assert_ne!(at(0, 0), red, "a round corner");

        // the picture starts as the ground, cleared: nothing else drawn
        let bare = pixels(&mut d, &|_| {});
        assert!(
            bare.chunks(stride)
                .flat_map(|r| r[..2400].chunks(4))
                .all(|p| p == &bare[..4])
        );

        // what cannot act keeps its hit: a finger on it never opens the row's page
        let mut busy = Commands::new(
            Client::new(
                "http://127.0.0.1:9",
                &temp_dir("busy"),
                Duration::from_secs(1),
            ),
            Arc::new(|| {}),
        );
        let travelling = Order::one("k:busy", "x", Post::empty("/x"));
        busy.run(travelling.clone());
        busy.run(
            Order::one("k:timed", "x", Post::empty("/x"))
                .toggle(true)
                .within(Duration::from_secs(30))
                .timed(true),
        );
        let open = Some(Act::Open(crate::ui::model::View::Plug("p".into())));
        let row = Tile::new(
            "Prise",
            "Allumée",
            Lead::Glyph {
                icon: icon::PLUG,
                on: true,
            },
        )
        .open(open)
        .parts(vec![Part::Row(vec![
            Button::new(None, "Sans réponse", Act::Order(travelling)),
            Button::new(None, "Hors de portée", Act::Refresh(&[])).unavailable(true),
        ])]);
        let page = Screen::Page {
            title: "Prise".into(),
            back: Box::new(Button::new(
                None,
                "Retour",
                Act::Open(crate::ui::model::View::House),
            )),
            groups: vec![Group::new("Prise", Body::Tiles(vec![row]))],
        };
        let ctx_with = |commands, over| Ctx {
            pal: &LIGHT,
            words: &words,
            commands,
            sliders: &sliders,
            now: ctx.now,
            scale: 1.0,
            width: 800.0,
            height: 1280.0,
            scroll: 0.0,
            over,
            fade: 1.0,
        };
        let drawn = paint(&mut d, &ctx_with(&busy, Over::default()), &page, &[]).unwrap();
        let on = |hits: &[Hit], what: &str| {
            // the last drawn wins, as `App::under` reads them
            let covers: Vec<_> = hits
                .iter()
                .filter(|h| matches!(h.what, Target::Cover) && h.y > px::HEADER_H)
                .collect();
            assert_eq!(
                covers.len(),
                2,
                "{what}: both buttons keep their hit: {hits:?}"
            );
            let last = hits
                .iter()
                .rposition(|h| h.contains(covers[0].x + 5.0, covers[0].y + 5.0));
            assert!(matches!(hits[last.unwrap()].what, Target::Cover), "{what}");
        };
        on(&drawn.hits, "busy, unavailable");

        // Maison gone: one band, no line per group, every gesture out of reach, nothing shifted
        let stale_group = group.clone().stale(Some((
            "Dernière lecture".into(),
            Button::new(None, "Réessayer", Act::Refresh(&[])),
        )));
        let failed = Group::new(
            "Prises",
            Body::Failed {
                text: "Maison ne répond pas".into(),
                retry: Button::new(None, "Réessayer", Act::Refresh(&[])),
            },
        );
        let old = Screen::Page {
            title: "Maison".into(),
            back: Box::new(Button::new(
                None,
                "Retour",
                Act::Open(crate::ui::model::View::House),
            )),
            groups: vec![stale_group, failed],
        };
        let shown = paint(&mut d, &ctx, &old, &[]).unwrap();
        let gone = Over {
            gone: true,
            ..Over::default()
        };
        let toast = [toasts[0].clone()];
        let drawn = paint(&mut d, &ctx_with(&commands, gone), &old, &toast).unwrap();
        assert!(
            !drawn
                .hits
                .iter()
                .any(|h| h.what.key().is_some_and(|k| k.contains("lamp"))),
            "no gesture left: {:?}",
            drawn.hits
        );
        assert!(
            drawn
                .hits
                .iter()
                .any(|h| matches!(h.what, Target::Act(Act::Open(_)))),
            "the way back stays"
        );
        assert!(
            drawn.content < shown.content,
            "stale lines gone, nothing pushed down"
        );
        let band = drawn
            .hits
            .iter()
            .find(|h| matches!(h.what, Target::Cover) && h.w == 800.0 && h.y == 1280.0 - TOUCH);
        assert!(band.is_some(), "one band along the bottom");
        let toast_hit = drawn.hits.last().unwrap();
        assert!(
            toast_hit.y + toast_hit.h <= 1280.0 - TOUCH,
            "the toast above it"
        );

        // a dialog: nothing reached around it, its two answers
        let asked = crate::ui::model::Asked {
            title: words.s(k::FOUNTAIN_RESET_FILTER_TITLE),
            text: words.s(k::FOUNTAIN_RESET_DESCRIPTION),
            action: words.s(k::DEVICE_RESET),
            order: Order::one("r", "x", Post::empty("/x")),
        };
        let over = Over {
            asking: Some(&asked),
            ..Over::default()
        };
        let drawn = paint(&mut d, &ctx_with(&commands, over), &house, &[]).unwrap();
        let answers: Vec<_> = drawn.hits.iter().rev().take(2).collect();
        assert_eq!(answers[0].what.key().as_deref(), Some("dialog:sure"));
        assert_eq!(answers[1].what.key().as_deref(), Some("dialog:keep"));
        let top = drawn
            .hits
            .iter()
            .rposition(|h| h.contains(10.0, 640.0))
            .unwrap();
        assert!(matches!(drawn.hits[top].what, Target::Cover));

        // a press shows on any target, in a wash over its box
        let box_ = (100.0, 300.0, 200.0, 44.0);
        let plain = Over::default();
        let pressed = Over {
            pressed: Some(box_),
            ..plain
        };
        let read = |d: &mut Draw, over| {
            paint(d, &ctx_with(&commands, over), &house, &[]).unwrap();
            d.canvas
                .read_pixels(None)
                .unwrap()
                .with_lock(<[u8]>::to_vec)
        };
        assert!(read(&mut d, plain) != read(&mut d, pressed));

        // every part of a page draws, and a finger finds its keys
        let key = |name: &str, repeat| {
            let o = Order::one(format!("key:{name}"), "x", Post::empty("/x"));
            Button::new(Some(icon::CHEVRON_UP), "", Act::Key(o, repeat))
        };
        let pad = crate::ui::model::Pad {
            up: key("up", Some(Duration::from_millis(100))),
            left: key("left", None),
            ok: key("ok", None),
            right: key("right", None),
            down: key("down", None),
            under: vec![key("back", None)],
        };
        let month = crate::ui::model::Month {
            title: "Novembre 2026".into(),
            weekdays: ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
            lead: 6,
            days: [
                (Some(maison_api::tempo::Color::Red), None),
                (Some(maison_api::tempo::Color::White), None),
                (
                    Some(maison_api::tempo::Color::Blue),
                    Some(("81 %".to_string(), false)),
                ),
                (
                    Some(maison_api::tempo::Color::Red),
                    Some(("40 %".to_string(), true)),
                ),
                (None, None),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (color, forecast))| crate::ui::model::Day {
                day: i as u32 + 1,
                color,
                forecast,
                today: i == 1,
            })
            .collect(),
        };
        let parts = vec![
            Part::Pad(Box::new(pad)),
            Part::Flow(vec![
                Button::new(None, "Rouge", Act::Refresh(&[])).dot(Rgba(239, 68, 68, 255)),
                Button::new(None, "Bleu", Act::Refresh(&[])).primary(true),
            ]),
            Part::Facts(vec![("Tension".into(), "230,1 V".into())]),
            Part::Table {
                head: vec!["Échéance".into(), "Hiver".into()],
                rows: vec![vec!["J+1".into(), "90 %".into()]],
            },
            Part::Calendar(Box::new(month)),
            Part::Progress {
                key: "k:timed".into(),
                label: words.s(k::TV_WAKING),
                elapsed: k::TV_WAKING_ELAPSED,
                limit: Duration::from_secs(30),
            },
        ];
        let page = Screen::Page {
            title: "Page".into(),
            back: Box::new(Button::new(
                None,
                "Retour",
                Act::Open(crate::ui::model::View::House),
            )),
            groups: vec![Group::new("Pavé", Body::Parts(parts.clone()))],
        };
        let drawn = paint(&mut d, &ctx, &page, &[]).unwrap();
        let center = |k: &str| {
            let h = drawn
                .hits
                .iter()
                .find(|h| h.what.key().as_deref() == Some(k))
                .unwrap_or_else(|| panic!("{k}: {:?}", drawn.hits));
            (h.x + h.w / 2.0, h.y + h.h / 2.0, h.w)
        };
        let (ux, uy, uw) = center("key:up");
        let (ox, oy, ow) = center("key:ok");
        let (lx, ly, _) = center("key:left");
        assert_eq!((uw, ow), (px::REMOTE_KEY, px::REMOTE_OK));
        assert!((ux - ox).abs() < 1.0 && uy < oy, "the arrows around OK");
        assert!((ly - oy).abs() < 1.0 && lx < ox);
        // a timed order: its bar takes its place only while it travels
        let tall = |commands, d: &mut Draw| {
            paint(d, &ctx_with(commands, Over::default()), &page, &[])
                .unwrap()
                .content
        };
        let idle = tall(&commands, &mut d);
        assert!(tall(&busy, &mut d) > idle);

        // the door: its one button found
        let door = Screen::Door {
            title: words.s(k::SCREEN_PAIR_TITLE),
            lead: vec![words.s(k::SCREEN_PAIR_EXPIRED)],
            code: Some("ABCD-2345".into()),
            hint: None,
            button: Some(Box::new(Button::new(
                Some(icon::REFRESH_CW),
                words.s(k::SCREEN_PAIR_AGAIN),
                Act::Pairing(crate::pairing::Ask::Again),
            ))),
            warn: false,
        };
        let drawn = paint(&mut d, &ctx, &door, &[]).unwrap();
        assert!(matches!(
            drawn.hits.as_slice(),
            [h] if matches!(h.what, Target::Act(Act::Pairing(_)))
        ));

        // the photos: the new one comes in over the last, the time in a corner, nothing else
        let colour = |id: &str, rgb: [u8; 4]| crate::photos::Picture {
            id: id.into(),
            width: 8,
            height: 12,
            rgba: rgb.repeat(8 * 12),
        };
        d.gfx.stage(colour("a", [0, 0, 200, 255])).unwrap();
        d.gfx.stage(colour("b", [200, 0, 0, 255])).unwrap();
        let photos = Screen::Photos {
            time: "12:34".into(),
            photo: "b".into(),
        };
        let at_fade = |fade: f32, d: &mut Draw| {
            let over = Over {
                gone: true,
                ..Over::default()
            };
            let c = Ctx {
                fade,
                ..ctx_with(&commands, over)
            };
            let drawn = paint(d, &c, &photos, &toasts).unwrap();
            assert!(drawn.hits.is_empty(), "no target, no band, no toast");
            let s = d
                .canvas
                .read_pixels(None)
                .unwrap()
                .convert_format(sdl3::pixels::PixelFormat::RGBA32)
                .unwrap();
            let pitch = s.pitch() as usize;
            let pixel = |x: usize, y: usize| s.with_lock(|px| px[y * pitch + x * 4]);
            let middle = s.with_lock(|px| [px[640 * pitch + 1600], px[640 * pitch + 1602]]);
            (middle, pixel(790, 1270))
        };
        assert_eq!(at_fade(0.0, &mut d).0, [0, 200], "the last one, whole");
        let [r, b] = at_fade(0.5, &mut d).0;
        assert!((90..110).contains(&r) && (90..110).contains(&b), "{r} {b}");
        assert_eq!(at_fade(1.0, &mut d).0, [200, 0], "the new one, whole");
        d.gfx.settle();
        assert_eq!(at_fade(1.0, &mut d).0, [200, 0], "the last one is gone");
        d.gfx.clear_stage();
        let (middle, _) = at_fade(1.0, &mut d);
        assert_eq!(middle, [LIGHT.ground.0, LIGHT.ground.2], "nothing left");
        // a first photo comes in over what was cleared
        d.gfx.stage(colour("c", [200, 0, 0, 255])).unwrap();
        let [r, _] = at_fade(0.5, &mut d).0;
        assert!(r > 200, "{r}");

        // the late gesture's post found nobody: its answer comes back
        let mut said = Vec::new();
        wait_until(|| commands.take(&words, &mut |_| {}, &mut said));
        wait_until(|| busy.take(&words, &mut |_| {}, &mut said));
    }
}
