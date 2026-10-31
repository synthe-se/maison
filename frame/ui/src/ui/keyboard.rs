//! A touch keyboard, AZERTY: letters, digits, symbols, one shift, a backspace, on the frame's portrait
//! panel. The state is pure (what was typed, which layer, the shift), the keys drawn from it; a key is
//! an `Act::Keyboard`, typed on the finger's lift like any tap, and shows its press as any target does
//! (nothing of its own to animate: idle, it draws nothing). Whoever owns a `Keyboard` hands it its keys
//! and reads `text`. A press is logged by `App::key_under` as « a key », never which.

use std::borrow::Cow;

use crate::i18n::k;
use crate::theme::{Face, Type, px};
use crate::ui::gfx::Error;
use crate::ui::icon;
use crate::ui::layout::{P, Target};
use crate::ui::model::Act;

/// A key's height: a fingertip's, whatever the 15 to 30 px the touch panel is off by (`REACH`).
pub const KEY_H: f32 = 92.0;
pub const KEY_GAP: f32 = px::S_1;
/// Its words: read at arm's length, under a finger.
const KEY: Type = Type {
    weight: 500,
    size: 26.0,
    line: 32.0,
    face: Face::Text,
};
const KEY_ICON: f32 = 28.0;
const ROWS: usize = 4;
/// The side keys of a row of letters, in a letter's widths.
const SIDE: f32 = 1.5;
/// The layer key and the space bar.
const LAYER: f32 = 2.0;
const SPACE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Letters,
    Digits,
    Symbols,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    Off,
    /// The next letter.
    Once,
    Locked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Space,
    Back,
    Shift,
    Layer(Layer),
}

impl Key {
    /// Its name (a simulation's `press kb:a`).
    pub fn name(self) -> String {
        match self {
            Self::Char(c) => format!("kb:{c}"),
            Self::Space => "kb:space".into(),
            Self::Back => "kb:back".into(),
            Self::Shift => "kb:shift".into(),
            Self::Layer(Layer::Letters) => "kb:letters".into(),
            Self::Layer(Layer::Digits) => "kb:digits".into(),
            Self::Layer(Layer::Symbols) => "kb:symbols".into(),
        }
    }
}

/// One row's plain keys, then what lies around them.
struct Plan {
    first: &'static str,
    second: &'static str,
    /// The third row's, between its two side keys.
    third: &'static str,
    /// The third row's side key.
    side: Key,
    /// The last row's: its layer key, then two keys after the space bar.
    layer: Layer,
    ends: [char; 2],
}

fn plan(layer: Layer) -> Plan {
    match layer {
        Layer::Letters => Plan {
            first: "azertyuiop",
            second: "qsdfghjklm",
            third: "wxcvbn'",
            side: Key::Shift,
            layer: Layer::Digits,
            ends: ['-', '.'],
        },
        Layer::Digits => Plan {
            first: "1234567890",
            second: "@#&_+=/*%$",
            third: "()\":;!?",
            side: Key::Layer(Layer::Symbols),
            layer: Layer::Letters,
            ends: [',', '.'],
        },
        Layer::Symbols => Plan {
            first: "<>[]{}\\|~^",
            second: "`€£§°µéèàç",
            third: "ùêîôûëï",
            side: Key::Layer(Layer::Digits),
            layer: Layer::Letters,
            ends: ['\'', '-'],
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keyboard {
    text: String,
    /// The longest text taken, in bytes.
    max: usize,
    layer: Layer,
    shift: Shift,
}

impl Keyboard {
    pub fn new(max: usize) -> Self {
        Self {
            text: String::new(),
            max,
            layer: Layer::Letters,
            shift: Shift::Off,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// A character as a key would type it: past the limit, nothing.
    pub fn put(&mut self, c: char) {
        if self.text.len() + c.len_utf8() <= self.max {
            self.text.push(c);
        }
    }

    /// `text`, as keys would type it.
    pub fn set(&mut self, text: &str) {
        self.text.clear();
        for c in text.chars() {
            self.put(c);
        }
    }

    pub fn press(&mut self, key: Key) {
        match key {
            Key::Char(c) => {
                self.put(self.cased(c));
                if self.shift == Shift::Once {
                    self.shift = Shift::Off;
                }
            }
            Key::Space => self.put(' '),
            Key::Back => {
                self.text.pop();
            }
            Key::Shift => {
                self.shift = match self.shift {
                    Shift::Off => Shift::Once,
                    Shift::Once => Shift::Locked,
                    Shift::Locked => Shift::Off,
                }
            }
            Key::Layer(layer) => self.layer = layer,
        }
    }

    /// `c`, in the shift's case.
    fn cased(&self, c: char) -> char {
        if self.layer == Layer::Letters && self.shift != Shift::Off {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        }
    }

    /// The keys, row by row, each with its width in a letter's.
    fn rows(&self) -> [Vec<(Key, f32)>; ROWS] {
        let p = plan(self.layer);
        let plain = |row: &str| row.chars().map(|c| (Key::Char(c), 1.0)).collect::<Vec<_>>();
        let mut third = vec![(p.side, SIDE)];
        third.extend(plain(p.third));
        third.push((Key::Back, SIDE));
        let [left, right] = p.ends;
        [
            plain(p.first),
            plain(p.second),
            third,
            vec![
                (Key::Layer(p.layer), LAYER),
                (Key::Space, SPACE),
                (Key::Char(left), 1.0),
                (Key::Char(right), 1.0),
            ],
        ]
    }
}

/// The keyboard's height.
pub const fn height() -> f32 {
    ROWS as f32 * KEY_H + (ROWS - 1) as f32 * KEY_GAP
}

impl P<'_, '_, '_, '_> {
    /// The keys in `w` CSS px from (x, y), `height()` high; a row's widths fill it.
    pub fn keyboard(&mut self, kb: &Keyboard, x: f32, y: f32, w: f32) -> Result<(), Error> {
        for (r, row) in kb.rows().iter().enumerate() {
            let units: f32 = row.iter().map(|(_, u)| u).sum();
            let gaps = (row.len() - 1) as f32 * KEY_GAP;
            let unit = (w - gaps) / units;
            let ky = y + r as f32 * (KEY_H + KEY_GAP);
            let mut kx = x;
            for (key, u) in row {
                let kw = unit * u;
                self.key(kb, *key, kx, ky, kw)?;
                kx += kw + KEY_GAP;
            }
        }
        Ok(())
    }

    fn key(&mut self, kb: &Keyboard, key: Key, x: f32, y: f32, w: f32) -> Result<(), Error> {
        let pal = self.c.pal;
        let side = !matches!(key, Key::Char(_) | Key::Space);
        let on = key == Key::Shift && kb.shift != Shift::Off;
        let (fill, line, ink) = match (on, kb.shift) {
            (true, Shift::Locked) => (pal.accent, pal.accent, pal.on_accent),
            (true, _) => (pal.accent_wash, pal.accent, pal.ink),
            _ if side => (pal.ground_raised, pal.line, pal.ink),
            _ => (pal.surface, pal.line, pal.ink),
        };
        self.framed((x, y, w, KEY_H), px::RADIUS, fill, line)?;
        let words = self.c.words;
        let label: Cow<'_, str> = match key {
            Key::Char(c) => kb.cased(c).to_string().into(),
            Key::Space => words.s(k::KEYBOARD_SPACE).into(),
            Key::Layer(Layer::Letters) => words.s(k::KEYBOARD_LETTERS).into(),
            Key::Layer(Layer::Digits) => words.s(k::KEYBOARD_DIGITS).into(),
            Key::Layer(Layer::Symbols) => words.s(k::KEYBOARD_SYMBOLS).into(),
            Key::Back | Key::Shift => "".into(),
        };
        let glyph = match key {
            Key::Back => Some(icon::DELETE),
            Key::Shift => Some(icon::ARROW_UP),
            _ => None,
        };
        if let Some(glyph) = glyph {
            let (ix, iy) = (x + (w - KEY_ICON) / 2.0, y + (KEY_H - KEY_ICON) / 2.0);
            self.icon(glyph, ix, iy, KEY_ICON, ink)?;
        } else {
            let lw = self.width(&label, KEY)?;
            let ly = y + (KEY_H - KEY.line) / 2.0;
            self.line(&label, KEY, ink, x + (w - lw) / 2.0, ly, lw)?;
        }
        self.hit(x, y, w, KEY_H, Target::Act(Act::Keyboard(key)));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_word_goes_in_as_typed_and_comes_back_a_key_at_a_time() {
        let mut kb = Keyboard::new(8);
        for key in [Key::Char('a'), Key::Space, Key::Char('z')] {
            kb.press(key);
        }
        assert_eq!(kb.text(), "a z");
        kb.press(Key::Back);
        assert_eq!(kb.text(), "a ");
        kb.press(Key::Back);
        kb.press(Key::Back);
        kb.press(Key::Back);
        assert_eq!(kb.text(), "");
    }

    #[test]
    fn the_shift_serves_once_then_locks_then_lets_go() {
        let mut kb = Keyboard::new(16);
        kb.press(Key::Shift);
        kb.press(Key::Char('a'));
        kb.press(Key::Char('a'));
        assert_eq!(kb.text(), "Aa");
        kb.press(Key::Shift);
        kb.press(Key::Shift);
        kb.press(Key::Char('z'));
        kb.press(Key::Char('z'));
        kb.press(Key::Shift);
        kb.press(Key::Char('z'));
        assert_eq!(kb.text(), "AaZZz");
    }

    #[test]
    fn digits_and_symbols_ignore_the_shift() {
        let mut kb = Keyboard::new(16);
        kb.press(Key::Shift);
        kb.press(Key::Shift);
        kb.press(Key::Layer(Layer::Digits));
        kb.press(Key::Char('7'));
        kb.press(Key::Layer(Layer::Symbols));
        kb.press(Key::Char('é'));
        assert_eq!(kb.text(), "7é");
    }

    #[test]
    fn past_its_limit_it_takes_nothing_and_never_half_a_character() {
        let mut kb = Keyboard::new(3);
        kb.set("abcd");
        assert_eq!(kb.text(), "abc");
        kb.set("ab");
        kb.put('é');
        assert_eq!(kb.text(), "ab", "two bytes in a place for one");
    }

    #[test]
    fn every_printable_ascii_character_has_its_key() {
        let mut reachable = String::new();
        for layer in [Layer::Letters, Layer::Digits, Layer::Symbols] {
            let kb = Keyboard {
                layer,
                ..Keyboard::new(1)
            };
            for (key, _) in kb.rows().into_iter().flatten() {
                if let Key::Char(c) = key {
                    reachable.push(c);
                    reachable.extend(c.to_uppercase());
                }
            }
        }
        for c in '!'..='~' {
            assert!(reachable.contains(c), "{c}");
        }
    }

    #[test]
    fn every_row_of_every_layer_fills_ten_letters_and_every_way_back_is_there() {
        for layer in [Layer::Letters, Layer::Digits, Layer::Symbols] {
            let kb = Keyboard {
                layer,
                ..Keyboard::new(1)
            };
            for row in kb.rows() {
                assert_eq!(row.iter().map(|(_, u)| u).sum::<f32>(), 10.0, "{layer:?}");
            }
            let keys: Vec<Key> = kb.rows().into_iter().flatten().map(|(k, _)| k).collect();
            assert!(keys.contains(&Key::Back) && keys.contains(&Key::Space));
            let names: Vec<String> = keys.iter().map(|k| k.name()).collect();
            let mut unique = names.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), names.len(), "{layer:?}: a key twice");
        }
    }
}
