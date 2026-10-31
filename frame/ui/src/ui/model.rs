//! What a screen shows, said by the families from what they read (pure, tested without a
//! screen), drawn by `paint`: the web app's bricks, the same names (Group, DeviceTile, Range,
//! a button row, Loaded's states).

use std::borrow::Cow;
use std::time::Duration;

use crate::commands::Order;
use crate::pairing::Ask;
use crate::theme::Rgba;
use crate::ui::icon::Icon;
use crate::ui::keyboard::{Key, Keyboard};
use maison_api::tempo::Color;

/// A dashboard group (Group.svelte): a title, a fact or actions in its head, then its body.
#[derive(Debug, Clone)]
pub struct Group {
    /// What « Maintenant » leads to it by.
    pub anchor: Option<&'static str>,
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

impl Group {
    pub fn new(title: impl Into<String>, body: Body) -> Self {
        Self {
            anchor: None,
            title: title.into(),
            fact: None,
            actions: Vec::new(),
            stale: None,
            body,
        }
    }

    pub fn anchor(mut self, anchor: &'static str) -> Self {
        self.anchor = Some(anchor);
        self
    }

    pub fn fact(mut self, fact: Option<String>) -> Self {
        self.fact = fact;
        self
    }

    pub fn actions(mut self, actions: Vec<Button>) -> Self {
        self.actions = actions;
        self
    }

    pub fn stale(mut self, stale: Option<(String, Button)>) -> Self {
        self.stale = stale;
        self
    }
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
    /// A tile's parts, framed, without its head (a remote's pad, the calendar).
    Parts(Vec<Part>),
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

impl Tile {
    pub fn new(name: impl Into<String>, state: impl Into<String>, lead: Lead) -> Self {
        Self {
            name: name.into(),
            state: state.into(),
            warn: false,
            lead,
            fact: None,
            parts: Vec::new(),
            open: None,
        }
    }

    pub fn warn(mut self, warn: bool) -> Self {
        self.warn = warn;
        self
    }

    pub fn fact(mut self, fact: Option<String>) -> Self {
        self.fact = fact;
        self
    }

    pub fn parts(mut self, parts: Vec<Part>) -> Self {
        self.parts = parts;
        self
    }

    pub fn open(mut self, open: Option<Act>) -> Self {
        self.open = open;
        self
    }
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
    /// Buttons left to right, wrapping: a choice, the chosen one `primary` (ToggleGroup).
    Flow(Vec<Button>),
    /// A remote's pad (Pad.svelte).
    Pad(Box<Pad>),
    /// Label and value, a line each (StatusRow).
    Facts(Vec<(String, String)>),
    /// A bar on the elapsed time of the timed order `key`, only while it travels (Progress).
    Progress {
        key: String,
        label: String,
        /// What the elapsed `seconds` are said with.
        elapsed: &'static str,
        limit: Duration,
    },
    /// One month of Tempo days (TempoCalendar's table).
    Calendar(Box<Month>),
    /// A table: its head, its rows; the first column left, the others right.
    Table {
        head: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

/// Four arrows around an OK, the keys under them.
#[derive(Debug, Clone)]
pub struct Pad {
    pub up: Button,
    pub left: Button,
    pub ok: Button,
    pub right: Button,
    pub down: Button,
    pub under: Vec<Button>,
}

/// A month of the calendar: its title, how many empty cells before its first day.
#[derive(Debug, Clone, PartialEq)]
pub struct Month {
    pub title: String,
    pub weekdays: [&'static str; 7],
    pub lead: usize,
    pub days: Vec<Day>,
}

/// A calendar's cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub day: u32,
    pub color: Option<Color>,
    /// Not published by RTE: a forecast, its probability, whether it is unsure.
    pub forecast: Option<(String, bool)>,
    pub today: bool,
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
    /// A disc of this color before the label (a lamp's colors).
    pub dot: Option<Rgba>,
}

impl Button {
    pub fn new(icon: Option<Icon>, label: impl Into<String>, act: Act) -> Self {
        Self {
            icon,
            label: label.into(),
            act,
            primary: false,
            unavailable: false,
            dot: None,
        }
    }

    pub fn dot(mut self, color: Rgba) -> Self {
        self.dot = Some(color);
        self
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
            Act::Ask(a) => Some(&a.order.key),
            _ => None,
        }
    }
}

/// What cannot be undone, asked first (ConfirmDialog): the consequence, the precise verb.
#[derive(Debug, Clone)]
pub struct Asked {
    pub title: String,
    pub text: String,
    pub action: String,
    pub order: Order,
}

/// What a tap does.
#[derive(Debug, Clone)]
pub enum Act {
    Order(Order),
    /// A remote's key: shows nothing in flight, repeats this often while held.
    Key(Order, Option<Duration>),
    /// Asked first; the order goes on « Remettre à zéro », nothing on « Garder ».
    Ask(Box<Asked>),
    /// The dialog's two answers.
    Keep,
    Sure,
    /// « Maintenant »: a chip leads to its group, « +N » shows them all.
    Jump(&'static str),
    Unfold,
    /// The Tempo calendar's month (`year * 12 + month0`).
    Month(i32),
    /// The families' sources read again now (a failed group's « Réessayer »).
    Refresh(&'static [&'static str]),
    Pairing(Ask),
    /// Another screen of the house (the Tempo page, a cat device's); `View::House` is the way
    /// back.
    Open(View),
    Wifi(WifiAct),
    /// A key of the on-screen keyboard.
    Keyboard(Key),
}

/// What a tap does on the Wi-Fi's screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiAct {
    Open,
    Back,
    Rescan,
    /// A network of the list; `locked`: its password is asked first.
    Choose {
        ssid: String,
        locked: bool,
    },
    /// The network that failed, again.
    Retry,
    /// The password as typed, or hidden.
    Show,
    /// The password typed goes.
    Join,
}

impl WifiAct {
    /// Its name (a simulation's `press wifi:rescan`).
    pub fn key(&self) -> Cow<'_, str> {
        match self {
            Self::Open => "wifi:open".into(),
            Self::Back => "wifi:back".into(),
            Self::Rescan => "wifi:rescan".into(),
            Self::Choose { ssid, .. } => format!("wifi:network:{ssid}").into(),
            Self::Retry => "wifi:retry".into(),
            Self::Show => "wifi:show".into(),
            Self::Join => "wifi:join".into(),
        }
    }
}

/// A network's password being typed: its name, the field, the keyboard.
#[derive(Debug, Clone)]
pub struct Password {
    pub title: String,
    pub back: Box<Button>,
    pub label: String,
    /// What the field shows: bullets unless asked (`show`).
    pub field: String,
    /// Why it cannot go.
    pub refused: Option<String>,
    pub show: Box<Button>,
    pub join: Box<Button>,
    pub keyboard: Keyboard,
}

/// What the paired screen shows: the house, or one of its pages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    House,
    Tempo,
    /// A cat device's page, by its id (`/device/{id}` on the web).
    Device(String),
    Climate,
    Tv,
    Box,
    /// A lamp's page: its family's key, its id.
    Lamp(&'static str, String),
    Plug(String),
}

impl View {
    /// What opens it, by name (a simulation's `press open:tempo`, `press open:device:<id>`).
    pub fn key(&self) -> Cow<'static, str> {
        match self {
            Self::House => "open:house".into(),
            Self::Tempo => "open:tempo".into(),
            Self::Device(id) => format!("open:device:{id}").into(),
            Self::Climate => "open:climate".into(),
            Self::Tv => "open:tv".into(),
            Self::Box => "open:box".into(),
            Self::Lamp(family, id) => format!("open:{family}:{id}").into(),
            Self::Plug(id) => format!("open:plug:{id}").into(),
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
    /// Its order is a PUT (the TV's volume).
    pub put: bool,
    /// Shown, does nothing (a lamp that is off: its reason said under it).
    pub off: bool,
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
    /// The group it leads to.
    pub to: &'static str,
}

/// The whole screen.
#[derive(Debug, Clone)]
pub enum Screen {
    /// The house: the time and the date, « Maintenant », then its groups in the dashboard's order.
    House {
        time: String,
        date: String,
        now: Vec<Chip>,
        groups: Vec<Group>,
    },
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
        /// The way to the Wi-Fi, quiet under the rest.
        link: Option<Box<Button>>,
    },
    /// The networks in range, one column; no way back while the frame has no network.
    Wifi {
        title: String,
        /// How the network stands, in words.
        word: String,
        back: Option<Box<Button>>,
        groups: Vec<Group>,
    },
    Password(Box<Password>),
    /// Nobody at the frame: the family's photos and the time. Its pictures are the renderer's
    /// (`Gfx::stage`); `photo` names the one on top.
    Photos {
        time: String,
        photo: String,
    },
}
