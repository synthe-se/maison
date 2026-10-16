//! What the frame takes from the web app at build time, never copied by hand (AGENTS.md: one
//! logic, one place):
//!
//! - **tokens**: `web/src/styles/tokens.css` → the colors (light and dark, from `light-dark()`),
//!   the spacing, sizes, corners and the type scale, as Rust consts (`tokens.rs`);
//! - **icons**: the lucide paths of `web/src/lib/components/Icon.svelte` (`icons.rs`);
//! - **words**: one const per key of `i18n/messages/fr.json`, so a misspelt key does not build
//!   (`keys.rs`); the files themselves are embedded by `i18n.rs`, and both locales must have
//!   the same keys;
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

/// `#f2f0ea`, `#fff`, `transparent`.
fn color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    if s == "transparent" {
        return Some(Rgba([0, 0, 0, 0]));
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

/// The declarations of the first `:root { … }` that declares the palette: `--name` → value.
fn declarations(css: &str) -> BTreeMap<String, String> {
    // comments out first: they hold `;` and `:`
    let mut plain = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        plain.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |j| &rest[i + j + 2..]);
    }
    plain.push_str(rest);
    let mut found = BTreeMap::new();
    // only the top-level blocks: media queries change a few sizes for the web's breakpoints
    let mut depth = 0;
    let mut block = String::new();
    for c in plain.chars() {
        match c {
            '{' => {
                depth += 1;
                block.clear();
            }
            '}' => {
                if depth == 1 {
                    for decl in block.split(';') {
                        if let Some((k, v)) = decl.split_once(':') {
                            let k = k.trim();
                            if k.starts_with("--") {
                                found.entry(k.to_string()).or_insert(v.trim().to_string());
                            }
                        }
                    }
                }
                depth -= 1;
                block.clear();
            }
            _ => block.push(c),
        }
    }
    found
}

/// A value's light and dark colors: `#hex`, `var(--x)`, `light-dark(a, b)`, each side resolved.
fn resolve(all: &BTreeMap<String, String>, value: &str, depth: u8) -> Option<(Rgba, Rgba)> {
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
    if let Some(name) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) {
        return resolve(all, all.get(name.trim())?, depth + 1);
    }
    color(value).map(|c| (c, c))
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

/// `16px` → 16, `1.75rem` → 28 (the web's 16 px rem).
fn length(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("px") {
        return n.parse().ok();
    }
    if let Some(n) = s.strip_suffix("rem") {
        return n.parse::<f32>().ok().map(|n| n * 16.0);
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

fn tokens_rs(css: &str) -> Result<String> {
    let all = declarations(css);
    let mut colors = Vec::new();
    let mut lengths = Vec::new();
    let mut types = Vec::new();
    for (name, value) in &all {
        if name.starts_with("--raw-") {
            continue;
        }
        if let Some(t) = type_token(value) {
            types.push((ident(name).to_uppercase(), t));
        } else if let Some(px) = length(value).or_else(|| value.parse().ok()) {
            lengths.push((ident(name).to_uppercase(), px));
        } else if let Some(pair) = resolve(&all, value, 0) {
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
    ] {
        if !lengths.iter().any(|(n, _)| n == needed) {
            return Err(format!(
                "tokens.css: no length `--{}`",
                needed.to_lowercase().replace('_', "-")
            ));
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
    rs.push_str("}\n\n/// The type scale.\npub mod ty {\n    use super::{Face, Type};\n");
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

/// The `ICONS` table of Icon.svelte: `name: '<path …/>…'`, a name bare or quoted, its markup on
/// the same line or the next.
fn icons_rs(svelte: &str) -> Result<String> {
    let start = svelte
        .find("const ICONS = {")
        .ok_or("Icon.svelte: no `const ICONS = {`")?;
    let body = &svelte[start + "const ICONS = {".len()..];
    let end = body
        .find("} as const")
        .ok_or("Icon.svelte: no `} as const` after ICONS")?;
    let mut icons = BTreeMap::new();
    let mut rest = body[..end].trim_start();
    while !rest.is_empty() {
        let (name, after) = if let Some(r) = rest.strip_prefix('\'') {
            let i = r.find('\'').ok_or("Icon.svelte: an unclosed name")?;
            (&r[..i], &r[i + 1..])
        } else {
            let i = rest.find(':').ok_or("Icon.svelte: a name without `:`")?;
            (rest[..i].trim(), &rest[i..])
        };
        let after = after
            .trim_start()
            .strip_prefix(':')
            .ok_or(format!("Icon.svelte: `{name}` without `:`"))?;
        let after = after
            .trim_start()
            .strip_prefix('\'')
            .ok_or(format!("Icon.svelte: `{name}`'s markup is not quoted"))?;
        let i = after
            .find('\'')
            .ok_or(format!("Icon.svelte: `{name}`'s markup is not closed"))?;
        icons.insert(name.to_string(), after[..i].to_string());
        rest = after[i + 1..]
            .trim_start()
            .trim_start_matches(',')
            .trim_start();
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
         /// An icon's markup inside a 24×24 view box, stroked: `None` for a name the web app lacks.\n\
         pub fn markup(name: &str) -> Option<&'static str> {\n    Some(match name {\n",
    );
    for (n, m) in &icons {
        let _ = writeln!(rs, "        {n:?} => {m:?},");
    }
    rs.push_str("        _ => return None,\n    })\n}\n\n/// Every name, for the tests.\npub const NAMES: &[&str] = &[");
    for n in icons.keys() {
        let _ = write!(rs, "{n:?}, ");
    }
    rs.push_str("];\n");
    Ok(rs)
}

// ── words ──

fn keys_rs(fr: &str, en: &str) -> Result<String> {
    let keys = |json: &str, file: &str| -> Result<BTreeSet<String>> {
        let v: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("{file}: {e}"))?;
        let o = v.as_object().ok_or(format!("{file}: not an object"))?;
        Ok(o.keys().filter(|k| !k.starts_with('$')).cloned().collect())
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
