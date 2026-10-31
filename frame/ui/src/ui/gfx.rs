//! Drawing on the GPU through SDL's renderer (GLES 2 on Lima), in the panel's pixels: text from
//! SDL_ttf's renderer engine (its glyphs in an atlas: a line costs one draw, not a texture),
//! anti-aliased discs and rounded rectangles from one white texture per radius (its corners and
//! a solid middle: every piece of a box comes from it, one batch, tinted), the web app's icons
//! rasterised once per size (resvg), tinted. Everything drawn is cached: a line, a shape, an icon
//! or a picture is made once and drawn again from its texture; a text's size is kept apart, by
//! `TTF_GetStringSize`, so a line measured and not drawn makes nothing.

use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::hash::Hash;

use resvg::tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use resvg::usvg;
use sdl3::iostream::IOStream;
use sdl3::pixels::PixelFormat;
use sdl3::rect::Rect as SdlRect;
use sdl3::render::{BlendMode, Canvas, FRect, ScaleMode, Texture, TextureCreator};
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

/// Lines kept drawn; past this, those not drawn for a while go.
const TEXTS_KEPT: usize = 1200;
/// Sizes kept; past this they are measured again.
const SIZES_KEPT: usize = 4000;
/// The solid side between a shape texture's corners.
const MIDDLE: u32 = 2;

/// The value under `key`, made once. By reference: a look-up that finds it allocates nothing.
fn cached<'m, K, Q, V>(
    map: &'m mut HashMap<K, V>,
    key: &Q,
    make: impl FnOnce() -> Result<V, Error>,
) -> Result<&'m mut V, Error>
where
    K: Borrow<Q> + Eq + Hash,
    Q: ToOwned<Owned = K> + Eq + Hash + ?Sized,
{
    if !map.contains_key(key) {
        map.insert(key.to_owned(), make()?);
    }
    Ok(map.get_mut(key).expect("just inserted"))
}

/// A NUL would end the C string early: never in the API's words, dropped if it came.
fn clean(text: &str) -> Cow<'_, str> {
    if text.contains('\0') {
        text.replace('\0', "").into()
    } else {
        text.into()
    }
}

/// `true`, or SDL's error.
fn sdl(ok: bool) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(sdl3::get_error().into())
    }
}

/// In pixels.
type Size = (u32, u32);

pub(super) struct Line {
    text: Text,
    pub(super) size: Size,
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
    /// Each pen's lines, by their words (0 wide: one line).
    pub(super) lines: HashMap<(Style, u16, Rgba, i32), HashMap<String, Line>>,
    /// A text's size, whatever its color.
    sizes: HashMap<(Style, u16, i32), HashMap<String, Size>>,
    engine: TextEngine,
    fonts: HashMap<(Style, u16), Font<'static>>,
    /// By radius.
    shapes: HashMap<u32, Texture<'t>>,
    icons: HashMap<(Icon, u32), Texture<'t>>,
    /// Pictures drawn in their own colors (a Tempo swatch), by size, by their SVG.
    pub(super) pictures: HashMap<(u32, u32), HashMap<String, Texture<'t>>>,
    /// Pictures made from a small key (a Tempo mark): nothing is written to find one.
    marks: HashMap<(u32, u32), Texture<'t>>,
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
            sizes: HashMap::new(),
            engine: TextEngine::new(creator)?,
            fonts: HashMap::new(),
            shapes: HashMap::new(),
            icons: HashMap::new(),
            pictures: HashMap::new(),
            marks: HashMap::new(),
            creator,
            ttf,
            frame: 0,
        })
    }

    /// A frame starts; lines not drawn for long go when there are too many, the kept sizes all at
    /// once when they are (a clock's minutes add up).
    pub fn begin(&mut self) {
        self.frame += 1;
        if self.lines.values().map(HashMap::len).sum::<usize>() > TEXTS_KEPT {
            let recent = self.frame.saturating_sub(4);
            for lines in self.lines.values_mut() {
                lines.retain(|_, l| l.used >= recent);
            }
        }
        if self.sizes.values().map(HashMap::len).sum::<usize>() > SIZES_KEPT {
            self.sizes.clear();
        }
    }

    fn font(&mut self, style: Style, size: u16) -> Result<&Font<'static>, Error> {
        font(&mut self.fonts, &self.ttf, style, size)
    }

    /// The line, drawn once. `pen.wrap` 0: one line.
    pub(super) fn line(&mut self, pen: Pen, text: &str) -> Result<&Line, Error> {
        let (engine, fonts, ttf, frame) =
            (&mut self.engine, &mut self.fonts, &self.ttf, self.frame);
        let lines = self
            .lines
            .entry((pen.style, pen.size, pen.color, pen.wrap))
            .or_default();
        let line = cached(lines, text, || {
            // the font outlives the text (field order), the engine is this renderer's
            let font = font(fonts, ttf, pen.style, pen.size)?;
            let mut t = engine.create_text(font, &clean(text))?;
            t.set_color(pen.color)?;
            if pen.wrap > 0 {
                t.set_wrap_width(pen.wrap)?;
            }
            t.update()?;
            let size = t.size();
            Ok(Line {
                text: t,
                size,
                used: 0,
            })
        })?;
        line.used = frame;
        Ok(line)
    }

    /// A text's size, as it would be drawn: asked once of SDL_ttf, nothing made.
    pub fn measure(
        &mut self,
        style: Style,
        size: u16,
        wrap: i32,
        text: &str,
    ) -> Result<(u32, u32), Error> {
        let (fonts, ttf) = (&mut self.fonts, &self.ttf);
        let sizes = self.sizes.entry((style, size, wrap)).or_default();
        let measured = cached(sizes, text, || {
            let text = clean(text);
            if text.is_empty() {
                return Ok((0, 0));
            }
            let font = font(fonts, ttf, style, size)?;
            let (mut w, mut h) = (0, 0);
            let c = text.as_ptr().cast();
            // SAFETY: a live font, a text of `len` bytes (not 0, which means NUL-terminated), two
            // out-parameters
            sdl(unsafe {
                if wrap > 0 {
                    sdl3::ttf::sys::TTF_GetStringSizeWrapped(
                        font.raw(),
                        c,
                        text.len(),
                        wrap,
                        &mut w,
                        &mut h,
                    )
                } else {
                    sdl3::ttf::sys::TTF_GetStringSize(font.raw(), c, text.len(), &mut w, &mut h)
                }
            })?;
            Ok((w.max(0) as u32, h.max(0) as u32))
        })?;
        Ok(*measured)
    }

    /// The text's top-left at (x, y).
    pub fn text(&mut self, pen: Pen, text: &str, x: f32, y: f32) -> Result<(u32, u32), Error> {
        if text.is_empty() {
            return Ok((0, 0));
        }
        let line = self.line(pen, text)?;
        line.text.draw(x.round(), y.round())?;
        Ok(line.size)
    }

    /// The text at x, centred in a line box `box_h` high from `top`.
    pub fn text_in(
        &mut self,
        pen: Pen,
        text: &str,
        x: f32,
        top: f32,
        box_h: f32,
    ) -> Result<(u32, u32), Error> {
        if text.is_empty() {
            return Ok((0, 0));
        }
        let line = self.line(pen, text)?;
        let y = top + (box_h - line.size.1 as f32) / 2.0;
        line.text.draw(x.round(), y.round())?;
        Ok(line.size)
    }

    /// How much of `text` fits in `max` pixels; the rest is cut and an ellipsis said. A NUL
    /// (never in the API's words) is dropped first, as a drawn line drops it.
    pub fn fit(&mut self, style: Style, size: u16, text: &str, max: u32) -> Result<String, Error> {
        let text = clean(text);
        if self.measure(style, size, 0, &text)?.0 <= max {
            return Ok(text.into_owned());
        }
        let ellipsis = self.measure(style, size, 0, "…")?.0;
        let room = max.saturating_sub(ellipsis) as i32;
        // SDL_ttf reads a width of 0 as no limit at all
        if room == 0 {
            return Ok("…".to_string());
        }
        let font = self.font(style, size)?;
        let (mut w, mut len) = (0, 0usize);
        // SAFETY: a live font, a text of `len` bytes (wider than `max`: not empty), two
        // out-parameters
        sdl(unsafe {
            sdl3::ttf::sys::TTF_MeasureString(
                font.raw(),
                text.as_ptr().cast(),
                text.len(),
                room,
                &mut w,
                &mut len,
            )
        })?;
        let mut cut = len.min(text.len());
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!("{}…", text[..cut].trim_end()))
    }

    /// The white texture boxes of radius `r` are drawn from: the corners of a disc, `MIDDLE`
    /// solid pixels between them. Sampled by the nearest pixel, its pieces meet without a seam.
    fn shape(&mut self, r: u32) -> Result<&mut Texture<'t>, Error> {
        let creator = self.creator;
        cached(&mut self.shapes, &r, || {
            let d = 2 * r;
            let mut pm = Pixmap::new(d, d).ok_or("a disc of no size")?;
            let c = r as f32;
            let path = PathBuilder::from_circle(c, c, c).ok_or("a disc of no size")?;
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
            let side = d + MIDDLE;
            let mut data = vec![255; (side * side * 4) as usize];
            let disc = pm.data().as_chunks::<4>().0;
            let of_disc = |v: u32| {
                (v < r)
                    .then_some(v)
                    .or((v >= r + MIDDLE).then(|| v - MIDDLE))
            };
            for (i, px) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (x, y) = (i as u32 % side, i as u32 / side);
                if let (Some(x), Some(y)) = (of_disc(x), of_disc(y)) {
                    px[3] = disc[(y * d + x) as usize][3];
                }
            }
            let mut t = texture(creator, &data, side, side)?;
            t.set_scale_mode(ScaleMode::Nearest);
            Ok(t)
        })
    }

    fn icon_texture(&mut self, icon: Icon, size: u32) -> Result<&mut Texture<'t>, Error> {
        let creator = self.creator;
        cached(&mut self.icons, &(icon, size), || {
            white_texture(creator, rasterise_icon(icon.markup(), size)?)
        })
    }

    pub(super) fn picture(&mut self, svg: &str, w: u32, h: u32) -> Result<&Texture<'t>, Error> {
        let creator = self.creator;
        let pictures = self.pictures.entry((w, h)).or_default();
        let picture = cached(pictures, svg, || rasterise(creator, svg, w, h))?;
        Ok(picture)
    }

    /// The picture `svg()` makes, kept under `key` at side `size`.
    fn mark(
        &mut self,
        key: u32,
        size: u32,
        svg: impl FnOnce() -> String,
    ) -> Result<&Texture<'t>, Error> {
        let creator = self.creator;
        let mark = cached(&mut self.marks, &(key, size), || {
            rasterise(creator, &svg(), size, size)
        })?;
        Ok(mark)
    }
}

/// An SVG on a `w` × `h` texture.
fn rasterise<'t>(
    creator: &'t TextureCreator<WindowContext>,
    svg: &str,
    w: u32,
    h: u32,
) -> Result<Texture<'t>, Error> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default())?;
    let mut pm = Pixmap::new(w, h).ok_or("a picture of no size")?;
    let size = tree.size();
    resvg::render(
        &tree,
        Transform::from_scale(w as f32 / size.width(), h as f32 / size.height()),
        &mut pm.as_mut(),
    );
    straight_texture(creator, pm)
}

/// The embedded face at `style`, loaded once.
fn font<'f>(
    fonts: &'f mut HashMap<(Style, u16), Font<'static>>,
    ttf: &Sdl3TtfContext,
    style: Style,
    size: u16,
) -> Result<&'f Font<'static>, Error> {
    let font = cached(fonts, &(style, size), || {
        let bytes = IOStream::from_bytes(font_bytes(style))?;
        Ok(ttf.load_font_from_iostream(bytes, f32::from(size))?)
    })?;
    Ok(font)
}

fn texture<'t>(
    creator: &'t TextureCreator<WindowContext>,
    data: &[u8],
    w: u32,
    h: u32,
) -> Result<Texture<'t>, Error> {
    let mut t = creator.create_texture_static(PixelFormat::RGBA32, w, h)?;
    t.update(None::<SdlRect>, data, w as usize * 4)?;
    t.set_blend_mode(BlendMode::Blend);
    Ok(t)
}

/// tiny-skia's premultiplied pixels made straight, as SDL blends them.
fn straight_texture<'t>(
    creator: &'t TextureCreator<WindowContext>,
    pm: Pixmap,
) -> Result<Texture<'t>, Error> {
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
    texture(creator, &data, w, h)
}

/// A white mask (tiny-skia's premultiplied pixels made straight: white, its coverage as
/// alpha), tinted when drawn.
fn white_texture<'t>(
    creator: &'t TextureCreator<WindowContext>,
    pm: Pixmap,
) -> Result<Texture<'t>, Error> {
    let (w, h) = (pm.width(), pm.height());
    let mut data = pm.take();
    for px in data.as_chunks_mut::<4>().0 {
        px[..3].fill(255);
    }
    texture(creator, &data, w, h)
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

/// A box on the panel, in its pixels.
pub type Area = (i32, i32, i32, i32);

impl Draw<'_, '_> {
    /// The whole picture in `color`, nothing blended.
    pub fn clear(&mut self, color: Rgba) {
        self.canvas.set_draw_color(color);
        self.canvas.clear();
    }

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
        // a round pill whose straight sides vanish: only its corners
        let straight = [
            (x + r, y, w - 2 * r, r),
            (x + r, y + h - r, w - 2 * r, r),
            (x, y + r, w, h - 2 * r),
        ];
        self.shape((x, y, w, h), r, &straight, color)
    }

    /// The box's four corners of radius `r`, then `straight`, all from one texture: one batch.
    fn shape(
        &mut self,
        (x, y, w, h): Area,
        r: i32,
        straight: &[Area],
        color: Rgba,
    ) -> Result<(), Error> {
        let t = self.gfx.shape(r as u32)?;
        t.set_color_mod(color.0, color.1, color.2);
        t.set_alpha_mod(color.3);
        let (rf, far) = (r as f32, (r as u32 + MIDDLE) as f32);
        for (sx, sy, dx, dy) in [
            (0.0, 0.0, x, y),
            (far, 0.0, x + w - r, y),
            (0.0, far, x, y + h - r),
            (far, far, x + w - r, y + h - r),
        ] {
            self.canvas.copy(
                t,
                FRect::new(sx, sy, rf, rf),
                FRect::new(dx as f32, dy as f32, rf, rf),
            )?;
        }
        let solid = FRect::new(rf, rf, MIDDLE as f32, MIDDLE as f32);
        for &(sx, sy, sw, sh) in straight {
            if sw > 0 && sh > 0 {
                let to = FRect::new(sx as f32, sy as f32, sw as f32, sh as f32);
                self.canvas.copy(t, solid, to)?;
            }
        }
        Ok(())
    }

    /// A rounded box with a ring `t` thick just inside its edge. Over an opaque fill only the
    /// ring's own band is drawn, not the whole box again: the same pixels, half the overdraw.
    pub fn ringed(
        &mut self,
        (x, y, w, h): Area,
        radius: i32,
        t: i32,
        fill: Rgba,
        ring: Rgba,
    ) -> Result<(), Error> {
        let r = radius.min(w / 2).min(h / 2).max(0);
        if fill.3 == 255 && ring.3 > 0 && r > 0 && t < w / 2 && t < h / 2 {
            let band = [
                (x + r, y, w - 2 * r, t),
                (x + r, y + h - t, w - 2 * r, t),
                (x, y + r, t, h - 2 * r),
                (x + w - t, y + r, t, h - 2 * r),
            ];
            self.shape((x, y, w, h), r, &band, ring)?;
        } else {
            self.rounded(x, y, w, h, radius, ring)?;
        }
        self.rounded(
            x + t,
            y + t,
            w - 2 * t,
            h - 2 * t,
            (radius - t).max(0),
            fill,
        )
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

    /// The picture kept under `key`, `side` square at (x, y); `svg` written only the first time.
    pub fn mark(
        &mut self,
        key: u32,
        side: u32,
        svg: impl FnOnce() -> String,
        (x, y): (i32, i32),
    ) -> Result<(), Error> {
        let t = self.gfx.mark(key, side, svg)?;
        let box_ = FRect::new(x as f32, y as f32, side as f32, side as f32);
        self.canvas.copy(t, None, box_)?;
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

    /// Every character the frame says is in the text face (the display face draws titles only).
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
        // the web app's plug page's « ≈ » and TV keyboard's arrows: words the frame never says
        let web_only: std::collections::BTreeSet<char> = ['←', '→', '≈'].into();
        assert_eq!(
            missing, web_only,
            "signs Cal Sans lacks: nothing draws them"
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
