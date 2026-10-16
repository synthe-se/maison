//! Drawing on the GPU through SDL's renderer (GLES 2 on Lima), in the panel's pixels: text from
//! SDL_ttf's renderer engine (its glyphs in an atlas: a line costs one draw, not a texture),
//! anti-aliased discs and rounded rectangles from one white disc per radius (drawn as nine
//! pieces, tinted), the web app's icons rasterised once per size (resvg), tinted. Everything
//! drawn is cached: a line, a disc, an icon or a picture is made once and drawn again from its
//! texture (looking one up still builds its key: a few small allocations a line, no texture).

use std::collections::HashMap;
use std::ffi::CString;
use std::path::Path;

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use resvg::usvg;
use sdl3::iostream::IOStream;
use sdl3::pixels::PixelFormat;
use sdl3::rect::Rect as SdlRect;
use sdl3::render::{BlendMode, Canvas, FRect, Texture, TextureCreator};
use sdl3::ttf::{Font, Sdl3TtfContext, Text, TextEngine};
use sdl3::video::{Window, WindowContext};

use crate::theme::{Face, Rgba, Style};
use crate::ui::icon::Icon;

/// The web app's faces, at the weights its type scale draws (build.rs instanced them).
static FONTS: [(Face, u16, &[u8]); 6] = [
    (
        Face::Text,
        400,
        include_bytes!(concat!(env!("OUT_DIR"), "/text-400.ttf")),
    ),
    (
        Face::Text,
        500,
        include_bytes!(concat!(env!("OUT_DIR"), "/text-500.ttf")),
    ),
    (
        Face::Text,
        600,
        include_bytes!(concat!(env!("OUT_DIR"), "/text-600.ttf")),
    ),
    (
        Face::Display,
        500,
        include_bytes!(concat!(env!("OUT_DIR"), "/display-500.ttf")),
    ),
    (
        Face::Display,
        600,
        include_bytes!(concat!(env!("OUT_DIR"), "/display-600.ttf")),
    ),
    (
        Face::Brand,
        400,
        include_bytes!(concat!(env!("OUT_DIR"), "/brand-400.ttf")),
    ),
];

/// The face's embedded weight nearest to `style`'s.
fn font_bytes(style: Style) -> &'static [u8] {
    FONTS
        .iter()
        .filter(|(f, _, _)| *f == style.face)
        .min_by_key(|(_, w, _)| w.abs_diff(style.weight))
        .map(|(_, _, b)| *b)
        .expect("every face is embedded")
}

/// Behind the text face, on the frame (Debian's fonts-noto-core, rootfs.sh): the signs Cal Sans
/// lacks (`FROM_FALLBACK`).
const FALLBACK_FONT: &str = "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf";

/// Lines kept drawn; past this, those not drawn for a while go.
const TEXTS_KEPT: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextKey {
    style: Style,
    size: u16,
    color: Rgba,
    /// 0: one line.
    wrap: i32,
    text: String,
}

struct Line {
    text: Text,
    size: (u32, u32),
    used: u64,
}

pub type Error = Box<dyn std::error::Error>;

/// How a line is drawn: its face and weight, its size in pixels, its color, and the width it
/// wraps at (0: one line).
#[derive(Debug, Clone, Copy)]
pub struct Pen {
    pub style: Style,
    pub size: u16,
    pub color: Rgba,
    pub wrap: i32,
}

/// The renderer's caches. Field order is drop order: the texts before their engine and their
/// fonts, the textures before the renderer that made them (held by the caller).
pub struct Gfx<'t> {
    lines: HashMap<TextKey, Line>,
    engine: TextEngine,
    fonts: HashMap<(Style, u16), Font<'static>>,
    /// The system's Noto Sans behind the text face, for the few signs Cal Sans lacks; dropped
    /// after the fonts that fall back on it.
    fallbacks: HashMap<u16, Font<'static>>,
    discs: HashMap<u32, Texture<'t>>,
    icons: HashMap<(Icon, u32), Texture<'t>>,
    /// Pictures drawn in their own colors (a Tempo swatch), by their SVG.
    pictures: HashMap<String, Texture<'t>>,
    creator: &'t TextureCreator<WindowContext>,
    ttf: Sdl3TtfContext,
    frame: u64,
}

impl<'t> Gfx<'t> {
    pub fn new(
        ttf: Sdl3TtfContext,
        creator: &'t TextureCreator<WindowContext>,
    ) -> Result<Self, Error> {
        Ok(Self {
            lines: HashMap::new(),
            engine: TextEngine::new(creator)?,
            fonts: HashMap::new(),
            fallbacks: HashMap::new(),
            discs: HashMap::new(),
            icons: HashMap::new(),
            pictures: HashMap::new(),
            creator,
            ttf,
            frame: 0,
        })
    }

    /// A frame starts; lines not drawn for long go when there are too many.
    pub fn begin(&mut self) {
        self.frame += 1;
        if self.lines.len() > TEXTS_KEPT {
            let recent = self.frame.saturating_sub(4);
            self.lines.retain(|_, l| l.used >= recent);
        }
    }

    fn font(&mut self, style: Style, size: u16) -> Result<&Font<'static>, Error> {
        self.load_font(style, size)?;
        Ok(&self.fonts[&(style, size)])
    }

    fn load_font(&mut self, style: Style, size: u16) -> Result<(), Error> {
        if !self.fonts.contains_key(&(style, size)) {
            let bytes = font_bytes(style);
            let font = self
                .ttf
                .load_font_from_iostream(IOStream::from_bytes(bytes)?, f32::from(size))?;
            if style.face == Face::Text && Path::new(FALLBACK_FONT).exists() {
                if !self.fallbacks.contains_key(&size) {
                    let fallback = self.ttf.load_font(FALLBACK_FONT, f32::from(size))?;
                    self.fallbacks.insert(size, fallback);
                }
                let fallback = &self.fallbacks[&size];
                // SAFETY: two live fonts; the fallback outlives the font (field order)
                if !unsafe { sdl3::ttf::sys::TTF_AddFallbackFont(font.raw(), fallback.raw()) } {
                    return Err(sdl3::get_error().into());
                }
            }
            self.fonts.insert((style, size), font);
        }
        Ok(())
    }

    /// The line drawn (or measured): its size in pixels. `wrap` 0: one line.
    fn line(
        &mut self,
        style: Style,
        size: u16,
        color: Rgba,
        wrap: i32,
        text: &str,
    ) -> Result<&Line, Error> {
        let key = TextKey {
            style,
            size,
            color,
            wrap,
            text: text.to_string(),
        };
        if !self.lines.contains_key(&key) {
            // a NUL would end the C string early: never in the API's words, dropped if it came
            let clean = text.replace('\0', "");
            self.load_font(style, size)?;
            let font = &self.fonts[&(style, size)];
            // the font outlives the text (field order), the engine is this renderer's
            let mut t = self.engine.create_text(font, &clean)?;
            t.set_color(color)?;
            if wrap > 0 {
                t.set_wrap_width(wrap)?;
            }
            t.update()?;
            let size = t.size();
            self.lines.insert(
                key.clone(),
                Line {
                    text: t,
                    size,
                    used: 0,
                },
            );
        }
        let frame = self.frame;
        let line = self.lines.get_mut(&key).expect("just inserted");
        line.used = frame;
        Ok(line)
    }

    /// A text's size, as it would be drawn.
    pub fn measure(
        &mut self,
        style: Style,
        size: u16,
        wrap: i32,
        text: &str,
    ) -> Result<(u32, u32), Error> {
        // measured in a color it may be drawn in: the line is reused when it is
        Ok(self.line(style, size, Rgba(0, 0, 0, 255), wrap, text)?.size)
    }

    /// The text's top-left at (x, y).
    pub fn text(&mut self, pen: Pen, text: &str, x: f32, y: f32) -> Result<(u32, u32), Error> {
        if text.is_empty() {
            return Ok((0, 0));
        }
        let line = self.line(pen.style, pen.size, pen.color, pen.wrap, text)?;
        line.text.draw(x.round(), y.round())?;
        Ok(line.size)
    }

    /// How much of `text` fits in `max` pixels; the rest is cut and an ellipsis said. A NUL
    /// (never in the API's words) is dropped first, as a drawn line drops it.
    pub fn fit(&mut self, style: Style, size: u16, text: &str, max: u32) -> Result<String, Error> {
        let text = text.replace('\0', "");
        if self.measure(style, size, 0, &text)?.0 <= max {
            return Ok(text);
        }
        let ellipsis = self.measure(style, size, 0, "…")?.0;
        let room = max.saturating_sub(ellipsis) as i32;
        let font = self.font(style, size)?;
        // no NUL left: the C string is the whole text, and `len` counts its bytes
        let c = CString::new(text.as_str())?;
        let (mut w, mut len) = (0, 0usize);
        // SAFETY: a live font, a NUL-terminated string and its length, two out-parameters
        let ok = unsafe {
            sdl3::ttf::sys::TTF_MeasureString(
                font.raw(),
                c.as_ptr(),
                c.as_bytes().len(),
                room,
                &mut w,
                &mut len,
            )
        };
        if !ok {
            return Err(sdl3::get_error().into());
        }
        let mut cut = len.min(text.len());
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!("{}…", text[..cut].trim_end()))
    }

    fn disc(&mut self, diameter: u32) -> Result<&mut Texture<'t>, Error> {
        if !self.discs.contains_key(&diameter) {
            let mut pm = Pixmap::new(diameter, diameter).ok_or("a disc of no size")?;
            let r = diameter as f32 / 2.0;
            let path = PathBuilder::from_circle(r, r, r).ok_or("a disc of no size")?;
            let mut paint = Paint::default();
            paint.set_color_rgba8(255, 255, 255, 255);
            paint.anti_alias = true;
            pm.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
            let t = self.white_texture(pm)?;
            self.discs.insert(diameter, t);
        }
        Ok(self.discs.get_mut(&diameter).expect("just inserted"))
    }

    fn icon_texture(&mut self, icon: Icon, size: u32) -> Result<&mut Texture<'t>, Error> {
        if !self.icons.contains_key(&(icon, size)) {
            let pm = rasterise_icon(icon.markup(), size)?;
            let t = self.white_texture(pm)?;
            self.icons.insert((icon, size), t);
        }
        Ok(self.icons.get_mut(&(icon, size)).expect("just inserted"))
    }

    fn picture(&mut self, svg: &str, w: u32, h: u32) -> Result<&Texture<'t>, Error> {
        if !self.pictures.contains_key(svg) {
            let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
            let mut pm = Pixmap::new(w, h).ok_or("a picture of no size")?;
            let size = tree.size();
            resvg::render(
                &tree,
                Transform::from_scale(w as f32 / size.width(), h as f32 / size.height()),
                &mut pm.as_mut(),
            );
            let t = self.straight_texture(pm)?;
            self.pictures.insert(svg.to_string(), t);
        }
        Ok(&self.pictures[svg])
    }

    /// tiny-skia's premultiplied pixels made straight, as SDL blends them.
    fn straight_texture(&self, pm: Pixmap) -> Result<Texture<'t>, Error> {
        let (w, h) = (pm.width(), pm.height());
        let mut data = pm.take();
        for px in data.as_chunks_mut::<4>().0 {
            let a = u16::from(px[3]);
            for c in &mut px[..3] {
                // a transparent pixel stays black
                *c = (u16::from(*c) * 255 + a / 2)
                    .checked_div(a)
                    .map_or(0, |v| v.min(255) as u8);
            }
        }
        self.texture(&data, w, h)
    }

    fn texture(&self, data: &[u8], w: u32, h: u32) -> Result<Texture<'t>, Error> {
        let mut t = self
            .creator
            .create_texture_static(PixelFormat::RGBA32, w, h)?;
        t.update(None::<SdlRect>, data, w as usize * 4)?;
        t.set_blend_mode(BlendMode::Blend);
        Ok(t)
    }

    /// A white mask (tiny-skia's premultiplied pixels made straight: white, its coverage as
    /// alpha), tinted when drawn.
    fn white_texture(&self, pm: Pixmap) -> Result<Texture<'t>, Error> {
        let (w, h) = (pm.width(), pm.height());
        let mut data = pm.take();
        for px in data.as_chunks_mut::<4>().0 {
            px[..3].fill(255);
        }
        self.texture(&data, w, h)
    }
}

/// An icon's markup as the web app draws it (24×24, stroked 2, round caps), white, `size` px.
pub fn rasterise_icon(markup: &str, size: u32) -> Result<Pixmap, Error> {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{markup}</svg>"##
    );
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default())?;
    let mut pm = Pixmap::new(size, size).ok_or("an icon of no size")?;
    let s = size as f32 / 24.0;
    resvg::render(&tree, Transform::from_scale(s, s), &mut pm.as_mut());
    Ok(pm)
}

/// What draws: the canvas and the caches, together.
pub struct Draw<'a, 't> {
    pub canvas: &'a mut Canvas<Window>,
    pub gfx: &'a mut Gfx<'t>,
}

impl Draw<'_, '_> {
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, color: Rgba) -> Result<(), Error> {
        if w <= 0 || h <= 0 || color.3 == 0 {
            return Ok(());
        }
        self.canvas.set_draw_color(color);
        self.canvas
            .fill_rect(FRect::new(x as f32, y as f32, w as f32, h as f32))?;
        Ok(())
    }

    /// A rectangle with round corners of `radius` (a disc when it is half the side).
    pub fn rounded(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        color: Rgba,
    ) -> Result<(), Error> {
        if w <= 0 || h <= 0 || color.3 == 0 {
            return Ok(());
        }
        let r = radius.min(w / 2).min(h / 2).max(0);
        if r == 0 {
            return self.fill(x, y, w, h, color);
        }
        let disc = self.gfx.disc(2 * r as u32)?;
        disc.set_color_mod(color.0, color.1, color.2);
        disc.set_alpha_mod(color.3);
        let rf = r as f32;
        for (sx, sy, dx, dy) in [
            (0, 0, x, y),
            (r, 0, x + w - r, y),
            (0, r, x, y + h - r),
            (r, r, x + w - r, y + h - r),
        ] {
            self.canvas.copy(
                disc,
                FRect::new(sx as f32, sy as f32, rf, rf),
                FRect::new(dx as f32, dy as f32, rf, rf),
            )?;
        }
        // a round pill whose straight sides vanish: only the disc's halves
        self.fill(x + r, y, w - 2 * r, r, color)?;
        self.fill(x + r, y + h - r, w - 2 * r, r, color)?;
        self.fill(x, y + r, w, h - 2 * r, color)
    }

    /// The icon, `size` px square at (x, y), in `color`.
    pub fn icon(
        &mut self,
        icon: Icon,
        x: i32,
        y: i32,
        size: u32,
        color: Rgba,
    ) -> Result<(), Error> {
        let canvas = &mut *self.canvas;
        let t = self.gfx.icon_texture(icon, size)?;
        t.set_color_mod(color.0, color.1, color.2);
        t.set_alpha_mod(color.3);
        canvas.copy(
            t,
            None,
            FRect::new(x as f32, y as f32, size as f32, size as f32),
        )?;
        Ok(())
    }

    pub fn text(&mut self, pen: Pen, text: &str, x: f32, y: f32) -> Result<(u32, u32), Error> {
        self.gfx.text(pen, text, x, y)
    }

    /// An SVG drawn in its own colors into (x, y, w, h), rasterised once.
    pub fn picture(&mut self, svg: &str, x: i32, y: i32, w: u32, h: u32) -> Result<(), Error> {
        let t = self.gfx.picture(svg, w, h)?;
        self.canvas
            .copy(t, None, FRect::new(x as f32, y as f32, w as f32, h as f32))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_of_the_web_app_draws() {
        for icon in crate::ui::icon::ALL {
            let name = icon.name();
            let pm = rasterise_icon(icon.markup(), 24).unwrap();
            let inked = pm
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] > 0)
                .count();
            assert!(inked > 10, "{name} draws {inked} pixels");
            assert_eq!(Icon::named(name), Some(*icon));
        }
        assert!(Icon::named("no-such-icon").is_none());
        // a scene's icons, as the API says them, are all the web app's
        for name in maison_api::scenes::ICONS {
            assert!(Icon::named(name).is_some(), "{name}");
        }
    }

    /// Every character of the words is in the text face (the display face draws titles only).
    #[test]
    fn the_text_font_has_every_character_of_the_words() {
        let ttf = sdl3::ttf::init().unwrap();
        let font = ttf
            .load_font_from_iostream(IOStream::from_bytes(FONTS[0].2).unwrap(), 16.0)
            .unwrap();
        let mut missing = std::collections::BTreeSet::new();
        for words in [
            include_str!(env!("MAISON_I18N_FR")),
            include_str!(env!("MAISON_I18N_EN")),
        ] {
            for c in words.chars().filter(|c| !c.is_control()) {
                // SAFETY: a live font
                if !unsafe { sdl3::ttf::sys::TTF_FontHasGlyph(font.raw(), u32::from(c)) } {
                    missing.insert(c);
                }
            }
        }
        // the plug page's « ≈ » and the TV keyboard's arrows, none on the frame: Noto Sans has them
        let from_fallback: std::collections::BTreeSet<char> = ['←', '→', '≈'].into();
        assert_eq!(
            missing, from_fallback,
            "signs Cal Sans lacks: add them to the fallback's"
        );
    }

    #[test]
    fn the_fonts_are_truetype() {
        for (_, _, font) in FONTS {
            // sfnt version 1.0 or `OTTO`; never `wOF2`
            assert!(
                font.starts_with(&[0, 1, 0, 0])
                    || font.starts_with(b"OTTO")
                    || font.starts_with(b"true")
            );
            assert!(font.len() > 20_000);
        }
    }
}
