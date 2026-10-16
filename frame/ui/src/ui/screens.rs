//! The screens, drawn from the bricks: the house (the header, then each group: its head, its
//! « old » line, its body), the door (a screen not paired: the brand, words, a code, a button),
//! and the toasts over both.

use crate::commands::Toast;
use crate::i18n::k;
use crate::theme::{px, ty};
use crate::ui::gfx::{Draw, Error};
use crate::ui::icon;
use crate::ui::layout::{
    BRAND_NUDGE, CHIP_ICON, CODE_SPACING, COLUMN_GAP, Ctx, Drawn, GROUP_GAP, GROUP_HEAD, HEAD_GAP,
    HEADER_MARK, MARK, MARK_SVG, NOW_LINES, P, STALE_ICON, TOUCH, balance, columns, fold, margin,
};
use crate::ui::model::{Body, Button, Chip, Group, Screen};

/// What the header says: the brand (the house), or a page's way back and its title.
enum Head<'a> {
    Brand,
    Back(&'a Button, &'a str),
}

impl P<'_, '_, '_, '_> {
    /// A group at (x, y), `w` wide (Group.svelte); its height.
    fn group(&mut self, g: &Group, x: f32, y: f32, w: f32) -> Result<f32, Error> {
        self.group_head(g, x, y, w)?;
        let mut cy = y + GROUP_HEAD + HEAD_GAP;
        if let Some((said, retry)) = &g.stale {
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
        Ok(())
    }

    /// « Maintenant » at (x, y), `w` wide: its chips on two lines at most, the rest behind
    /// « +N »; its height.
    fn now_strip(&mut self, chips: &[Chip], x: f32, y: f32, w: f32) -> Result<f32, Error> {
        let pal = self.c.pal;
        let h = px::CONTROL_H_XS;
        let gap = px::S_1;
        let mut widths = Vec::with_capacity(chips.len());
        for c in chips {
            let icon = if c.warn { CHIP_ICON + px::S_1 } else { 0.0 };
            widths.push(self.width(&c.text, ty::TINY)? + 2.0 * px::S_2 + icon);
        }
        let mut mores = Vec::new();
        for n in 0..=chips.len() {
            mores.push(self.width(&format!("+{n}"), ty::TINY)? + 2.0 * px::S_2);
        }
        let warn: Vec<bool> = chips.iter().map(|c| c.warn).collect();
        let (shown, folded) = fold(&widths, &warn, w, gap, NOW_LINES, |n| mores[n]);
        let more = format!("+{folded}");
        let pills = chips
            .iter()
            .zip(&widths)
            .zip(&shown)
            .filter(|(_, s)| **s)
            .map(|((c, cw), _)| (c.text.as_str(), c.warn, *cw))
            .chain((folded > 0).then(|| (more.as_str(), false, mores[folded])));
        let (mut cx, mut cy) = (x, y);
        for (text, warn, cw) in pills {
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
            let mut tx = cx + px::S_2;
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
            self.line(text, ty::TINY, ink, tx, cy + (h - ty::TINY.line) / 2.0, cw)?;
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
        match body {
            Body::Loading => {
                // a row's place, held (Loaded's skeleton: a gesture and its padding)
                let th = px::TILE_ICON + 2.0 * px::S_1;
                self.rounded(x, cy, w, th, px::RADIUS_L, pal.ground_raised)?;
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
            Body::Buttons(buttons) => cy += self.flow(buttons, x, cy, w)?,
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
        let mut top = px::HEADER_H + px::S_5;
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
                y += h + GROUP_GAP;
            }
            bottom = bottom.max(y - GROUP_GAP);
        }
        let content = (bottom + px::S_5) * self.s();
        // the header stays, a line under it: the brand, or a page's way back and its title
        self.dy = 0.0;
        self.rounded(0.0, 0.0, wcss, px::HEADER_H, 0.0, pal.ground)?;
        let hair = 1.0 / self.s();
        self.rounded(0.0, px::HEADER_H - hair, wcss, hair, 0.0, pal.line)?;
        if let Head::Back(back, title) = head {
            let bw = self.button_width(back)?;
            self.button(back, m, (px::HEADER_H - TOUCH) / 2.0, bw, TOUCH)?;
            let tx = m + bw + px::S_4;
            self.line(
                title,
                ty::GROUP,
                pal.ink,
                tx,
                (px::HEADER_H - ty::GROUP.line) / 2.0,
                wcss - tx - m,
            )?;
            return Ok(content);
        }
        let my = (px::HEADER_H - HEADER_MARK) / 2.0;
        if self.visible(my, HEADER_MARK) {
            let mark = self.d(HEADER_MARK).max(1) as u32;
            self.d
                .picture(MARK_SVG, self.d(m), self.y(my), mark, mark)?;
        }
        let bx = m + HEADER_MARK + px::S_2;
        let brand = self.c.words.s(k::BRANDING_NAME);
        self.line(
            &brand,
            ty::BRAND,
            pal.ink,
            bx,
            (px::HEADER_H - ty::BRAND.line) / 2.0 + BRAND_NUDGE * ty::BRAND.size,
            wcss - bx - m,
        )?;
        Ok(content)
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
    fn toasts(&mut self, toasts: &[Toast], wcss: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        self.dy = 0.0;
        let tw = (wcss - px::S_6).min(px::TOAST_W);
        let x = (wcss - tw) / 2.0;
        let text_w = tw - px::S_4 - px::S_3;
        let mut y = self.c.height / self.s() - px::S_4;
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
            y -= px::S_2;
        }
        Ok(())
    }
}

/// Draws the screen; what a finger can land on, and the column's height.
pub fn paint(d: &mut Draw, c: &Ctx, screen: &Screen, toasts: &[Toast]) -> Result<Drawn, Error> {
    let (w, h) = (c.width.round() as i32, c.height.round() as i32);
    d.fill(0, 0, w, h, c.pal.ground)?;
    let mut p = P {
        d,
        c,
        hits: Vec::new(),
        dy: -c.scroll,
        measuring: false,
    };
    let wcss = c.width / c.scale;
    let content = match screen {
        Screen::House { now, groups } => p.house(now, groups, Head::Brand, wcss)?,
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
    };
    p.toasts(toasts, wcss)?;
    Ok(Drawn {
        hits: p.hits,
        content,
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
    use crate::commands::{Commands, Order};
    use crate::families::lamps::{HUE_DRIVER, Lamp, lamps_group};
    use crate::i18n::{Lang, Words};
    use crate::live::View;
    use crate::testing::{temp_dir, wait_until};
    use crate::theme::{Face, LIGHT, Style};
    use crate::ui::gfx::Gfx;
    use crate::ui::layout::Target;
    use crate::ui::model::{Act, Lead};
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

        // fit: cut with an ellipsis within its room; a NUL dropped, never cutting words short
        let style = Style {
            face: Face::Text,
            weight: 400,
        };
        let long = "Une lampe au nom bien trop long pour sa tuile";
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
            now: vec![Chip {
                text: "2 lampes allumées".into(),
                warn: false,
            }],
            groups: vec![group.clone()],
        };
        let drawn = paint(&mut d, &ctx, &house, &toasts).unwrap();
        let keys: Vec<&str> = drawn.hits.iter().filter_map(|h| h.what.key()).collect();
        // « Tout éteindre », each gesture (its row's head and its disc), the lit one's slider,
        // and the late one's « Réessayer », which turns it on again
        assert!(keys.contains(&"lamps:all-off"), "{keys:?}");
        assert_eq!(
            keys.iter().filter(|k| **k == "hue-lamp:2:power").count(),
            3,
            "the late lamp's row, its disc and its « Réessayer »: {keys:?}"
        );
        let row = drawn
            .hits
            .iter()
            .find(|h| h.what.key() == Some("hue-lamp:1:power"))
            .unwrap();
        assert!(row.w > 300.0, "the whole row is the lamp's on/off: {row:?}");
        assert!(keys.contains(&"hue-lamp:1:power") && keys.contains(&"hue-lamp:1:brightness"));
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

        // the state lines, as each gesture says it
        {
            let p = P {
                d: &mut d,
                c: &ctx,
                hits: Vec::new(),
                dy: 0.0,
                measuring: false,
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

        // the late gesture's post found nobody: its answer comes back
        let mut said = Vec::new();
        wait_until(|| commands.take(&words, &mut |_| {}, &mut said));
    }
}
