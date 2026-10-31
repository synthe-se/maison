//! What the frame takes from the web app at build time, never copied by hand (AGENTS.md: one
//! logic, one place):
//!
//! - **tokens**: `web/src/styles/tokens.css` → the colors (light and dark, from `light-dark()`),
//!   the spacing, sizes, corners and the type scale, the lengths its media queries change (the
//!   page margin from 600 px, the finger's control heights), and the red day's hatching, as
//!   Rust consts (`tokens.rs`);
//! - **icons**: the lucide paths of `web/src/lib/components/Icon.svelte`, one typed const per
//!   name, so a misspelt icon does not build (`icons.rs`);
//! - **words**: one const per key of `i18n/messages/fr.json`, so a misspelt key does not build
//!   (`keys.rs`); the files themselves are embedded by `i18n.rs`: both locales must have the same
//!   keys, and every message the shape `i18n.rs` reads (a text, or a plural on `count`);
//! - **fonts**: the web app's own (Cal Sans, Fraunces, Borel), WOFF2 → TrueType (SDL_ttf's
//!   FreeType reads no WOFF2), the variable ones instanced at each weight the type scale draws
//!   (SDL_ttf cannot set an axis), embedded by `ui/gfx.rs`.
//!
//! Anything missing or unreadable stops the build with what to do.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::{env, fs};

type Result<T> = std::result::Result<T, String>;

fn main() {
    if let Err(e) = run() {
        // a build script's failure is its message: nothing else would say what went wrong
        eprintln!("maison-frame-ui build: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let repo = manifest
        .join("../..")
        .canonicalize()
        .map_err(|e| format!("the repository root: {e}"))?;
    let out = PathBuf::from(env::var("OUT_DIR").map_err(|e| e.to_string())?);

    let tokens = repo.join("web/src/styles/tokens.css");
    write(&out.join("tokens.rs"), &tokens_rs(&read(&tokens)?)?)?;
    let icons = repo.join("web/src/lib/components/Icon.svelte");
    write(&out.join("icons.rs"), &icons_rs(&read(&icons)?)?)?;
    let fr = repo.join("i18n/messages/fr.json");
    let en = repo.join("i18n/messages/en.json");
    write(&out.join("keys.rs"), &keys_rs(&read(&fr)?, &read(&en)?)?)?;

    let text = repo.join("web/static/fonts/CalSansVF.woff2");
    // Fraunces is the web app's dependency (fontsource), read where bun installs it
    let display = repo.join(
        "web/node_modules/@fontsource-variable/fraunces/files/fraunces-latin-wght-normal.woff2",
    );
    let brand = repo.join("web/static/fonts/BorelDisplay-Regular.woff2");
    // every weight tokens.css draws a face at (`--t-*`), and the tile title's 600
    let fonts: [(&str, &PathBuf, Option<f32>); 6] = [
        ("text-400.ttf", &text, Some(400.0)),
        ("text-500.ttf", &text, Some(500.0)),
        ("text-600.ttf", &text, Some(600.0)),
        ("display-500.ttf", &display, Some(500.0)),
        ("display-600.ttf", &display, Some(600.0)),
        ("brand-400.ttf", &brand, None),
    ];
    for (name, woff2, weight) in fonts {
        let raw = fs::read(woff2).map_err(|e| {
            format!(
                "{}: {e} (the web app's fonts: `bun install` in web/)",
                woff2.display()
            )
        })?;
        let ttf = truetype(&raw, weight).map_err(|e| format!("{}: {e}", woff2.display()))?;
        write(&out.join(name), &ttf)?;
        println!("cargo:rerun-if-changed={}", woff2.display());
    }
    for p in [&tokens, &icons, &fr, &en] {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rustc-env=MAISON_I18N_FR={}", fr.display());
    println!("cargo:rustc-env=MAISON_I18N_EN={}", en.display());
    Ok(())
}

/// A WOFF2 font as TrueType; a variable one instanced at `weight` (its other axes at their
/// defaults).
fn truetype(woff2: &[u8], weight: Option<f32>) -> Result<Vec<u8>> {
    use allsorts::binary::read::ReadScope;
    use allsorts::tables::variable_fonts::fvar::FvarTable;
    use allsorts::tables::{Fixed, FontTableProvider};
    use allsorts::tag;
    use allsorts::woff2::Woff2Font;
    let font = ReadScope::new(woff2)
        .read::<Woff2Font>()
        .map_err(|e| e.to_string())?;
    let provider = font.table_provider(0).map_err(|e| e.to_string())?;
    let Some(weight) = weight else {
        let tags = provider.table_tags().ok_or("no table directory")?;
        return allsorts::subset::whole_font(&provider, &tags).map_err(|e| e.to_string());
    };
    let fvar_data = provider
        .read_table_data(tag::FVAR)
        .map_err(|e| format!("not variable: {e}"))?;
    let fvar = ReadScope::new(&fvar_data)
        .read::<FvarTable>()
        .map_err(|e| e.to_string())?;
    let mut has_weight = false;
    let coords: Vec<Fixed> = fvar
        .axes()
        .map(|a| {
            if a.axis_tag == tag::WGHT {
                has_weight = true;
                Fixed::from(weight)
            } else {
                a.default_value
            }
        })
        .collect();
    if !has_weight {
        return Err("no weight axis".into());
    }
    let (ttf, _) = allsorts::variations::instance(&provider, &coords)
        .map_err(|e| format!("instance at {weight}: {e}"))?;
    Ok(ttf)
}

fn read(p: &Path) -> Result<String> {
    fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn write(p: &Path, contents: impl AsRef<[u8]>) -> Result<()> {
    fs::write(p, contents).map_err(|e| format!("{}: {e}", p.display()))
}

/// `--ground-raised` → `GROUND_RAISED`, `ground_raised`.
fn ident(name: &str) -> String {
    name.trim_start_matches("--").replace('-', "_")
}

// ── tokens ──

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rgba([u8; 4]);

/// `#f2f0ea`, `#fff`, `transparent`, `rgb(21 26 33 / 45%)`.
fn color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if s == "transparent" {
        return Some(Rgba([0, 0, 0, 0]));
    }
    if let Some(args) = s.strip_prefix("rgb(").and_then(|a| a.strip_suffix(')')) {
        let (rgb, alpha) = args.split_once('/')?;
        let mut n = rgb.split_whitespace().map(str::parse::<u8>);
        let (r, g, b) = (n.next()?.ok()?, n.next()?.ok()?, n.next()?.ok()?);
        let percent: f32 = alpha.trim().strip_suffix('%')?.parse().ok()?;
        return Some(Rgba([r, g, b, (percent * 2.55).round() as u8]));
    }
    let hex = s.strip_prefix('#')?;
    let hex: String = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect(),
        6 => hex.into(),
        _ => return None,
    };
    let v = u32::from_str_radix(&hex, 16).ok()?;
    Some(Rgba([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]))
}

type Declarations = BTreeMap<String, String>;

/// What tokens.css declares: on `:root`, and what its media queries change there.
#[derive(Default)]
struct Css {
    /// `:root { … }`, the later declaration of a name winning (the cascade's order).
    root: Declarations,
    /// `@media (min-width: Npx) { :root { … } }`, by N.
    wide: BTreeMap<u32, Declarations>,
    /// `@media (pointer: coarse) { :root { … } }`: a finger's, the frame's.
    coarse: Declarations,
}

/// The CSS without its comments (they hold `;`, `:` and braces).
fn uncommented(css: &str) -> String {
    let mut plain = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        plain.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |j| &rest[i + j + 2..]);
    }
    plain.push_str(rest);
    plain
}

/// `--name: value;` pairs of a block's own text.
fn custom_properties(block: &str, into: &mut Declarations) {
    for decl in block.split(';') {
        if let Some((k, v)) = decl.split_once(':') {
            let k = k.trim();
            if k.starts_with("--") {
                into.insert(k.to_string(), v.trim().to_string());
            }
        }
    }
}

fn declarations(css: &str) -> Result<Css> {
    let plain = uncommented(css);
    let mut found = Css::default();
    // the open blocks' preludes, outermost first, and the text read since the last brace
    let mut open: Vec<String> = Vec::new();
    let mut text = String::new();
    for c in plain.chars() {
        match c {
            '{' => {
                // a prelude is what follows the last declaration or block
                let prelude = text
                    .rsplit(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                open.push(prelude);
                text.clear();
            }
            '}' => {
                let preludes: Vec<&str> = open.iter().map(String::as_str).collect();
                match preludes.as_slice() {
                    [":root"] => custom_properties(&text, &mut found.root),
                    [media, ":root"] => {
                        let query = media.strip_prefix("@media").map(str::trim).ok_or(format!(
                            "tokens.css: `{media} {{ :root }}`: not a media query"
                        ))?;
                        if let Some(w) = query
                            .strip_prefix("(min-width:")
                            .and_then(|q| q.strip_suffix(')'))
                        {
                            let w =
                                length(w).ok_or(format!("tokens.css: `{query}`: a width in px"))?;
                            custom_properties(&text, found.wide.entry(w as u32).or_default());
                        } else if query == "(pointer: coarse)" {
                            custom_properties(&text, &mut found.coarse);
                        }
                        // other queries (a phone's narrow width) are no frame's: left out
                    }
                    _ => {}
                }
                open.pop();
                text.clear();
            }
            _ => text.push(c),
        }
    }
    if found.root.is_empty() {
        return Err("tokens.css: no `:root { … }` declares anything".into());
    }
    Ok(found)
}

/// A value's light and dark colors: `#hex`, `var(--x)`, `light-dark(a, b)`, each side resolved.
fn resolve(all: &Declarations, value: &str, depth: u8) -> Option<(Rgba, Rgba)> {
    if depth > 8 {
        return None;
    }
    let value = value.trim();
    if let Some(inner) = value
        .strip_prefix("light-dark(")
        .and_then(|v| v.strip_suffix(')'))
    {
        let (a, b) = split_top(inner)?;
        let (light, _) = resolve(all, a, depth + 1)?;
        let (_, dark) = resolve(all, b, depth + 1)?;
        return Some((light, dark));
    }
    if let Some(name) = var_name(value) {
        return resolve(all, all.get(name)?, depth + 1);
    }
    color(value).map(|c| (c, c))
}

/// `var(--x)` → `--x`.
fn var_name(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix("var(")
        .and_then(|v| v.strip_suffix(')'))
        .map(str::trim)
}

/// `a, b` at the top level of parentheses.
fn split_top(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

/// Every item of a comma list, at the top level of parentheses.
fn split_all(mut s: &str) -> Vec<&str> {
    let mut items = Vec::new();
    while let Some((a, b)) = split_top(s) {
        items.push(a.trim());
        s = b;
    }
    items.push(s.trim());
    items
}

/// `16px` → 16, `1.75rem` → 28 (the web's 16 px rem).
fn length(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("px") {
        return n.trim().parse().ok();
    }
    if let Some(n) = s.strip_suffix("rem") {
        return n.trim().parse::<f32>().ok().map(|n| n * 16.0);
    }
    None
}

/// `500 1.75rem/2.125rem var(--font-display)` → weight, size, line height, face.
fn type_token(s: &str) -> Option<(u16, f32, f32, &'static str)> {
    let mut parts = s.split_whitespace();
    let weight = parts.next()?.parse().ok()?;
    let (size, line) = parts.next()?.split_once('/')?;
    let size = length(size)?;
    // a unitless line height multiplies the size
    let line = length(line).or_else(|| line.parse::<f32>().ok().map(|l| l * size))?;
    let face = match parts.next()? {
        "var(--font-display)" => "Display",
        "var(--font-text)" => "Text",
        "var(--font-brand)" => "Brand",
        _ => return None,
    };
    Some((weight, size, line, face))
}

/// A red day's hatching, `--tempo-red-hatch`: `repeating-linear-gradient(45deg, var(--a) 0 3px,
/// color-mix(in srgb, var(--a) 70%, var(--b)) 3px 6px)` → the angle, the first stripe's end and
/// the period, the first color, the share of it in the mix, and what it mixes towards.
struct Hatch {
    angle: f32,
    stripe: f32,
    period: f32,
    first: String,
    mix: f32,
    towards: String,
}

fn hatch(value: &str) -> Result<Hatch> {
    let bad = |what: &str| format!("tokens.css: `--tempo-red-hatch: {value}`: {what}");
    let inner = value
        .trim()
        .strip_prefix("repeating-linear-gradient(")
        .and_then(|v| v.strip_suffix(')'))
        .ok_or_else(|| bad("not a repeating-linear-gradient()"))?;
    let [angle, one, two] = split_all(inner)[..] else {
        return Err(bad("an angle and two stops"));
    };
    let angle: f32 = angle
        .strip_suffix("deg")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| bad("an angle in deg"))?;
    // `var(--a) 0 3px`
    let mut one = one.split_whitespace();
    let first = one
        .next()
        .and_then(var_name)
        .ok_or_else(|| bad("a var() first"))?;
    let stripe = one
        .nth(1)
        .and_then(length)
        .ok_or_else(|| bad("the first stop's end"))?;
    // `color-mix(in srgb, var(--a) 70%, var(--b)) 3px 6px`
    let close = two.rfind(')').ok_or_else(|| bad("a color-mix()"))?;
    let (mix, ends) = two.split_at(close + 1);
    let period = ends
        .split_whitespace()
        .nth(1)
        .and_then(length)
        .ok_or_else(|| bad("the second stop's end"))?;
    let args = mix
        .strip_prefix("color-mix(")
        .and_then(|m| m.strip_suffix(')'))
        .ok_or_else(|| bad("a color-mix()"))?;
    let [space, mixed, towards] = split_all(args)[..] else {
        return Err(bad("color-mix(in srgb, a p%, b)"));
    };
    if space != "in srgb" {
        return Err(bad("a mix in srgb"));
    }
    let (mixed, share) = mixed.rsplit_once(' ').ok_or_else(|| bad("a share in %"))?;
    let share: f32 = share
        .strip_suffix('%')
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad("a share in %"))?;
    if var_name(mixed) != Some(first) {
        return Err(bad("the second stripe mixes the first's color"));
    }
    let towards = var_name(towards).ok_or_else(|| bad("a var() to mix towards"))?;
    Ok(Hatch {
        angle,
        stripe,
        period,
        first: ident(first),
        mix: share / 100.0,
        towards: ident(towards),
    })
}

/// The lengths of a block (a media query's), as consts.
fn lengths_of(decls: &Declarations) -> Vec<(String, f32)> {
    decls
        .iter()
        .filter_map(|(n, v)| Some((ident(n).to_uppercase(), length(v)?)))
        .collect()
}

fn tokens_rs(css: &str) -> Result<String> {
    let css = declarations(css)?;
    let all = &css.root;
    let mut colors = Vec::new();
    let mut lengths = Vec::new();
    let mut types = Vec::new();
    for (name, value) in all {
        if name.starts_with("--raw-") {
            continue;
        }
        if let Some(t) = type_token(value) {
            types.push((ident(name).to_uppercase(), t));
        } else if let Some(px) = length(value).or_else(|| value.parse().ok()) {
            lengths.push((ident(name).to_uppercase(), px));
        } else if let Some(pair) = resolve(all, value, 0) {
            colors.push((ident(name), pair));
        }
    }
    for needed in [
        "ground",
        "surface",
        "ink",
        "ink_muted",
        "accent",
        "accent_wash",
        "on_accent",
        "line",
        "blocked",
        "blocked_wash",
        "warn_text",
    ] {
        if !colors.iter().any(|(n, _)| n == needed) {
            return Err(format!(
                "tokens.css: no color `--{}`",
                needed.replace('_', "-")
            ));
        }
    }
    for needed in [
        "S_4",
        "RADIUS_L",
        "CONTROL_H",
        "TILE_ICON",
        "THUMB",
        "TRACK_H",
        "PAGE_MARGIN",
    ] {
        if !lengths.iter().any(|(n, _)| n == needed) {
            return Err(format!(
                "tokens.css: no length `--{}`",
                needed.to_lowercase().replace('_', "-")
            ));
        }
    }
    let hatch = hatch(
        all.get("--tempo-red-hatch")
            .ok_or("tokens.css: no `--tempo-red-hatch`")?,
    )?;
    for c in [&hatch.first, &hatch.towards] {
        if !colors.iter().any(|(n, _)| n == c) {
            return Err(format!("tokens.css: the hatch's `--{c}` is no color"));
        }
    }
    let mut rs =
        String::from("// Generated by build.rs from web/src/styles/tokens.css: never edited.\n\n");
    rs.push_str("/// Every color token, one side of `light-dark()`.\n#[derive(Debug, Clone, Copy, PartialEq)]\npub struct Palette {\n");
    for (n, _) in &colors {
        let _ = writeln!(rs, "    pub {n}: Rgba,");
    }
    rs.push_str("}\n\n");
    for (side, pick) in [("LIGHT", 0), ("DARK", 1)] {
        let _ = writeln!(rs, "pub const {side}: Palette = Palette {{");
        for (n, (l, d)) in &colors {
            let Rgba([r, g, b, a]) = if pick == 0 { *l } else { *d };
            let _ = writeln!(rs, "    {n}: Rgba({r}, {g}, {b}, {a}),");
        }
        rs.push_str("};\n\n");
    }
    rs.push_str("/// Lengths in the web's CSS pixels (scaled to the panel by `theme::Theme`), and the plain\n/// numbers (`DISABLED_OPACITY`).\npub mod px {\n");
    for (n, v) in &lengths {
        let _ = writeln!(rs, "    pub const {n}: f32 = {v:?};");
    }
    rs.push_str("}\n\n/// The lengths `@media (min-width: …)` changes: each, from which width (CSS px) it holds,\n/// narrowest first.\npub mod wide {\n");
    let mut wide: BTreeMap<String, Vec<(u32, f32)>> = BTreeMap::new();
    for (w, decls) in &css.wide {
        for (n, v) in lengths_of(decls) {
            wide.entry(n).or_default().push((*w, v));
        }
    }
    for (n, at) in &wide {
        let at: Vec<String> = at.iter().map(|(w, v)| format!("({w}.0, {v:?})")).collect();
        let _ = writeln!(
            rs,
            "    pub const {n}: &[(f32, f32)] = &[{}];",
            at.join(", ")
        );
    }
    rs.push_str("}\n\n/// The lengths `@media (pointer: coarse)` sets: a finger's, as on the frame.\npub mod coarse {\n");
    for (n, v) in lengths_of(&css.coarse) {
        let _ = writeln!(rs, "    pub const {n}: f32 = {v:?};");
    }
    let _ = write!(
        rs,
        "}}\n\n/// A red day's hatching (`--tempo-red-hatch`): stripes at `ANGLE`°, the first color to\n/// `STRIPE` px, then to `PERIOD` px the first mixed at `MIX` with what it is mixed towards.\npub mod hatch {{\n    use super::{{Palette, Rgba}};\n    pub const ANGLE: f32 = {:?};\n    pub const STRIPE: f32 = {:?};\n    pub const PERIOD: f32 = {:?};\n    pub const MIX: f32 = {:?};\n    pub fn first(p: &Palette) -> Rgba {{\n        p.{}\n    }}\n    pub fn towards(p: &Palette) -> Rgba {{\n        p.{}\n    }}\n}}\n\n",
        hatch.angle, hatch.stripe, hatch.period, hatch.mix, hatch.first, hatch.towards
    );
    rs.push_str("/// The type scale.\npub mod ty {\n    use super::{Face, Type};\n");
    for (n, (w, s, l, f)) in &types {
        let _ = writeln!(
            rs,
            "    pub const {}: Type = Type {{ weight: {w}, size: {s:?}, line: {l:?}, face: Face::{f} }};",
            n.trim_start_matches("T_")
        );
    }
    rs.push_str("}\n");
    Ok(rs)
}

// ── icons ──

/// A JavaScript string at the start of `s` (`'…'`, `"…"` or `` `…` ``, `\` escapes): its text
/// and what follows it.
fn js_string(s: &str) -> Option<(String, &str)> {
    let quote = s.chars().next().filter(|q| matches!(q, '\'' | '"' | '`'))?;
    let mut text = String::new();
    let mut chars = s.char_indices().skip(1);
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => text.push(chars.next()?.1),
            c if c == quote => return Some((text, &s[i + 1..])),
            c => text.push(c),
        }
    }
    None
}

/// `s` without leading blanks and `// …` comments.
fn skip_blank(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        match s.strip_prefix("//") {
            Some(rest) => s = rest.find('\n').map_or("", |i| &rest[i..]),
            None => return s,
        }
    }
}

/// The `ICONS` object of Icon.svelte: `name: '<path …/>…',` entries, a name bare or quoted, its
/// markup a string literal (any quote) on the same line or the next; the object ends at its
/// closing brace, whatever follows it (`as const`, a type).
fn icons_rs(svelte: &str) -> Result<String> {
    let start = svelte
        .find("const ICONS")
        .ok_or("Icon.svelte: no `const ICONS`")?;
    let open = svelte[start..]
        .find('{')
        .ok_or("Icon.svelte: no `{` after `const ICONS`")?;
    let mut rest = &svelte[start + open + 1..];
    let mut icons = BTreeMap::new();
    loop {
        rest = skip_blank(rest);
        if let Some(after) = rest.strip_prefix('}') {
            rest = after;
            break;
        }
        let (name, after) = match js_string(rest) {
            Some(quoted) => quoted,
            None => {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                    .ok_or("Icon.svelte: ICONS is not closed")?;
                (rest[..end].to_string(), &rest[end..])
            }
        };
        if name.is_empty() {
            return Err(format!(
                "Icon.svelte: not an entry: `{}`",
                &rest[..rest.len().min(40)]
            ));
        }
        let after = skip_blank(after)
            .strip_prefix(':')
            .ok_or(format!("Icon.svelte: `{name}` without `:`"))?;
        let (markup, after) = js_string(skip_blank(after))
            .ok_or(format!("Icon.svelte: `{name}`'s markup is not a string"))?;
        if icons.insert(name.clone(), markup).is_some() {
            return Err(format!("Icon.svelte: `{name}` twice"));
        }
        rest = skip_blank(after);
        rest = rest.strip_prefix(',').unwrap_or(rest);
    }
    if !skip_blank(rest).starts_with("as const") {
        return Err(
            "Icon.svelte: ICONS is no longer `as const`: its names would not be typed".into(),
        );
    }
    if icons.len() < 20 || !icons.contains_key("lightbulb") || !icons.contains_key("loader-circle")
    {
        return Err(format!(
            "Icon.svelte: {} icons read, `lightbulb` or `loader-circle` missing",
            icons.len()
        ));
    }
    let mut rs = String::from(
        "// Generated by build.rs from web/src/lib/components/Icon.svelte (lucide, ISC): never edited.\n\n\
         /// One of the web app's icons, its name and its markup: only these exist (a misspelt\n\
         /// name does not build).\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\n\
         pub struct Icon(&'static str, &'static str);\n\n\
         impl Icon {\n\
         \x20   /// Its name in the web app (`lightbulb-off`).\n\
         \x20   pub const fn name(self) -> &'static str {\n        self.0\n    }\n\n\
         \x20   /// The icon of this name, if the web app has it (a name from the API).\n\
         \x20   pub fn named(name: &str) -> Option<Self> {\n        ALL.iter().copied().find(|i| i.0 == name)\n    }\n\n\
         \x20   /// Its markup inside a 24×24 view box, stroked.\n\
         \x20   pub const fn markup(self) -> &'static str {\n        self.1\n    }\n}\n\n",
    );
    for (n, m) in &icons {
        let id = ident(n).to_uppercase();
        if !id.starts_with(|c: char| c.is_ascii_alphabetic()) {
            return Err(format!("Icon.svelte: `{n}` makes no Rust name"));
        }
        let _ = writeln!(rs, "pub const {id}: Icon = Icon({n:?}, {m:?});");
    }
    rs.push_str("\n/// Every icon.\npub const ALL: &[Icon] = &[");
    for n in icons.keys() {
        let _ = write!(rs, "{}, ", ident(n).to_uppercase());
    }
    rs.push_str("];\n");
    Ok(rs)
}

// ── words ──

/// A message as `i18n.rs` reads it: a text, or one plural on `count` (`local p = count:
/// plural`, `p=one` and `p=*`, both texts).
fn message_shape(key: &str, v: &serde_json::Value, file: &str) -> Result<()> {
    let bad = |what: &str| {
        format!("{file}: `{key}`: {what} (i18n.rs reads a text, or a plural on count)")
    };
    let Some(variants) = v.as_array() else {
        return if v.is_string() {
            Ok(())
        } else {
            Err(bad("neither a text nor a list"))
        };
    };
    let [one] = variants.as_slice() else {
        return Err(bad("more than one selector block"));
    };
    let o = one.as_object().ok_or_else(|| bad("not an object"))?;
    let strings = |field: &str| -> Result<Vec<&str>> {
        o.get(field)
            .and_then(|v| v.as_array())
            .ok_or_else(|| bad(&format!("no `{field}` list")))?
            .iter()
            .map(|d| {
                d.as_str()
                    .ok_or_else(|| bad(&format!("a `{field}` that is not text")))
            })
            .collect()
    };
    if !strings("declarations")?.contains(&"local p = count: plural") {
        return Err(bad("a selector other than `local p = count: plural`"));
    }
    if strings("selectors")? != ["p"] {
        return Err(bad("selectors other than `p`"));
    }
    let matches = o
        .get("match")
        .and_then(|m| m.as_object())
        .ok_or_else(|| bad("no `match`"))?;
    if !matches.contains_key("p=*") {
        return Err(bad("no `p=*`"));
    }
    for (k, t) in matches {
        if !matches!(k.as_str(), "p=one" | "p=*") {
            return Err(bad(&format!(
                "a variant `{k}`: only `p=one` and `p=*` are read"
            )));
        }
        if !t.is_string() {
            return Err(bad(&format!("`{k}` is not a text")));
        }
    }
    Ok(())
}

fn keys_rs(fr: &str, en: &str) -> Result<String> {
    let keys = |json: &str, file: &str| -> Result<BTreeSet<String>> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("{file}: {e}"))?;
        let o = v.as_object().ok_or(format!("{file}: not an object"))?;
        let mut keys = BTreeSet::new();
        for (k, m) in o.iter().filter(|(k, _)| !k.starts_with('$')) {
            message_shape(k, m, file)?;
            keys.insert(k.clone());
        }
        Ok(keys)
    };
    let fr = keys(fr, "fr.json")?;
    let en = keys(en, "en.json")?;
    if fr != en {
        let only: Vec<_> = fr.symmetric_difference(&en).take(5).collect();
        return Err(format!("fr.json and en.json differ: {only:?}"));
    }
    let mut rs =
        String::from("// Generated by build.rs from i18n/messages/fr.json: never edited.\n\n");
    for k in &fr {
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("fr.json: key `{k}` is not an identifier"));
        }
        let _ = writeln!(rs, "pub const {}: &str = {k:?};", k.to_uppercase());
    }
    Ok(rs)
}
