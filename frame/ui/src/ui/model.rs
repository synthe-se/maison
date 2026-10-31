//! What a screen shows, said by the families from what they read (pure, tested without a
//! screen), drawn by `paint`: the web app's bricks, the same names (Group, DeviceTile, Range,
//! a button row, Loaded's states).

use std::borrow::Cow;

use crate::commands::Order;
use crate::pairing::Ask;
use crate::ui::icon::Icon;
use maison_api::tempo::Color;

/// A dashboard group (Group.svelte): a title, a fact or actions in its head, then its body.
#[derive(Debug, Clone)]
pub struct Group {
    pub title: String,
    /// One short fact at the head's end (« 3 prises · 2 en ligne »).
    pub fact: Option<String>,
    /// Buttons at the head's end (« Tout éteindre »).
    pub actions: Vec<Button>,
    /// What it shows was read a while ago, or its last read failed (Stale.svelte): said, and
    /// « Réessayer ».
    pub stale: Option<(String, Button)>,
    pub body: Body,
}

/// Loaded.svelte's states, then the content.
#[derive(Debug, Clone)]
pub enum Body {
    /// First read on its way: a tile's shape holds the place.
    Loading,
    /// Nothing could be read: said, with a way to ask again.
    Failed {
        text: String,
        retry: Button,
    },
    /// Read, and nothing there.
    Empty {
        text: String,
        hint: String,
    },
    Tiles(Vec<Tile>),
    /// A row of buttons that wraps (the scenes).
    Buttons(Vec<Button>),
}

/// One device (DeviceTile.svelte).
#[derive(Debug, Clone)]
pub struct Tile {
    pub name: String,
    /// Line 2: the state in words.
    pub state: String,
    /// The state line in the warning color (unreachable, a red peak).
    pub warn: bool,
    pub lead: Lead,
    /// One short fact at the end (« 42 W »).
    pub fact: Option<String>,
    pub parts: Vec<Part>,
    /// The row leads somewhere (DeviceTile's `href`: the Tempo tile, its page).
    pub open: Option<Act>,
}

/// What leads the tile.
#[derive(Debug, Clone)]
pub enum Lead {
    /// The icon is the gesture: on/off, as the device said. A tap sends the order to the other
    /// side of what is shown (the target while one travels, as DeviceTile's `!shown`); both
    /// orders share one key, which holds the gesture's state.
    Toggle {
        icon: Icon,
        on: bool,
        offline: bool,
        turn_on: Box<Order>,
        turn_off: Box<Order>,
    },
    /// An icon that does nothing (on: in the accent wash).
    Glyph { icon: Icon, on: bool },
    /// A Tempo day's swatch: blue filled, white a ring, red hatched (Swatch.svelte).
    Swatch(Option<Color>),
}

/// What comes under the tile's head.
#[derive(Debug, Clone)]
pub enum Part {
    Slider(Slider),
    /// Equal buttons in a row (open · stop · close).
    Row(Vec<Button>),
    /// A line of words, muted, wrapping.
    Hint(String),
    /// Shares in Tempo's colors, one bar (a forecast's probabilities; a season's days used):
    /// each `(color, share)` in turn, the rest of the track empty. `thin`: Share.svelte's.
    Bar {
        shares: Vec<(Color, f64)>,
        thin: bool,
    },
    /// Words then a link-like button on one line (« Fermeture 20:24 · Ne pas fermer ce soir »).
    Note {
        text: String,
        link: Button,
        skipped: bool,
    },
}

/// A button (`.btn`).
#[derive(Debug, Clone)]
pub struct Button {
    pub icon: Option<Icon>,
    pub label: String,
    pub act: Act,
    /// `.btn.primary`: the accent, filled.
    pub primary: bool,
    /// Out of reach: shown, does nothing (its reason said on the tile).
    pub unavailable: bool,
}

impl Button {
    pub fn new(icon: Option<Icon>, label: impl Into<String>, act: Act) -> Self {
        Self {
            icon,
            label: label.into(),
            act,
            primary: false,
            unavailable: false,
        }
    }

    pub fn primary(mut self, yes: bool) -> Self {
        self.primary = yes;
        self
    }

    pub fn unavailable(mut self, yes: bool) -> Self {
        self.unavailable = yes;
        self
    }

    /// The gesture's key, if it sends an order (its state shows on the button: busy).
    pub fn key(&self) -> Option<&str> {
        match &self.act {
            Act::Order(o) => Some(&o.key),
            _ => None,
        }
    }
}

/// What a tap does.
#[derive(Debug, Clone)]
pub enum Act {
    Order(Order),
    /// The families' sources read again now (a failed group's « Réessayer »).
    Refresh(&'static [&'static str]),
    Pairing(Ask),
    /// Another screen of the house (the Tempo page, a cat device's); `View::House` is the way
    /// back.
    Open(View),
}

/// What the paired screen shows: the house, or one of its pages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    House,
    Tempo,
    /// A cat device's page, by its id (`/device/{id}` on the web).
    Device(String),
}

impl View {
    /// What opens it, by name (a simulation's `press open:tempo`, `press open:device:<id>`).
    pub fn key(&self) -> Cow<'static, str> {
        match self {
            Self::House => "open:house".into(),
            Self::Tempo => "open:tempo".into(),
            Self::Device(id) => format!("open:device:{id}").into(),
        }
    }
}

/// A slider (Range.svelte): a 44 px band, a thin track, the thumb.
#[derive(Debug, Clone)]
pub struct Slider {
    /// Its state is kept under this key (`hue-lamp:1:brightness`).
    pub key: String,
    /// What the device said.
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    /// Where the device is while it travels to the value (a shutter's position).
    pub mark: Option<f64>,
    /// Sends while it moves (at most every `SLIDER_THROTTLE_MS`), not only on release.
    pub live: bool,
    /// The value held after release is let go once the device says one this near.
    pub near: f64,
    /// Words under each end (« Fermé », « Ouvert »).
    pub ends: Option<(String, String)>,
    /// The order for a value: its path, and its body, one of the API's request types.
    pub path: String,
    pub body: fn(f64) -> serde_json::Value,
    pub refresh: &'static str,
    pub name: String,
}

/// A chip of « Maintenant » (NowStrip's Chip): a fact in words, what needs a hand in the
/// warning style.
#[derive(Debug, Clone, PartialEq)]
pub struct Chip {
    pub text: String,
    pub warn: bool,
}

/// The whole screen.
#[derive(Debug, Clone)]
pub enum Screen {
    /// The house: « Maintenant », then its groups in the dashboard's order.
    House { now: Vec<Chip>, groups: Vec<Group> },
    /// One of the house's pages (Tempo): its title, the way back, its groups in columns.
    Page {
        title: String,
        back: Box<Button>,
        groups: Vec<Group>,
    },
    /// A wall screen not (yet) paired, or a notice: a title, paragraphs, a big code, a button.
    Door {
        title: String,
        lead: Vec<String>,
        code: Option<String>,
        hint: Option<String>,
        button: Option<Box<Button>>,
        warn: bool,
    },
}
