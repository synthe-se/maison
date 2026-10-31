//! The web app's words (`i18n/messages/*.json`, the inlang message format), embedded: a key's
//! text with its `{params}` filled, its plural picked as Intl.PluralRules does for the
//! language (the only selector the files use: `local p = count: plural`, `p=one` / `p=*`),
//! and numbers written as the web app's `#lib/i18n.svelte.ts` writes them.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Timelike};
use serde::Deserialize;

/// One const per key (generated from fr.json): `t(k::LAMPS_ON_PERCENT, …)`.
#[allow(dead_code)] // every key of the files, the frame says some
pub mod k {
    include!(concat!(env!("OUT_DIR"), "/keys.rs"));
}

const FR: &str = include_str!(env!("MAISON_I18N_FR"));
const EN: &str = include_str!(env!("MAISON_I18N_EN"));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Fr,
    En,
}

impl Lang {
    /// `fr`, `fr_FR.UTF-8`, `en-GB`: anything not English is French (the house's language).
    pub fn from_tag(tag: &str) -> Self {
        if tag.to_ascii_lowercase().starts_with("en") {
            Self::En
        } else {
            Self::Fr
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Message {
    Plain(String),
    Plural([Variants; 1]),
}

#[derive(Debug, Deserialize)]
struct Variants {
    #[serde(rename = "match")]
    variants: HashMap<String, String>,
}

/// One locale's texts.
pub struct Words {
    lang: Lang,
    messages: HashMap<String, Message>,
}

/// A parameter: text, or a number (also what a plural is picked by).
#[derive(Debug, Clone)]
pub enum Arg {
    Text(String),
    Count(f64),
}

impl From<&str> for Arg {
    fn from(s: &str) -> Self {
        Self::Text(s.into())
    }
}
impl From<String> for Arg {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}
impl From<&String> for Arg {
    fn from(s: &String) -> Self {
        Self::Text(s.clone())
    }
}

impl Words {
    pub fn new(lang: Lang) -> Self {
        let json = match lang {
            Lang::Fr => FR,
            Lang::En => EN,
        };
        // build.rs refused to build unless both files are JSON and every message is a text or
        // a plural on `count` (`message_shape`): these cannot fail on the embedded files
        let mut messages: HashMap<String, serde_json::Value> =
            serde_json::from_str(json).expect("the messages are JSON (build.rs read them)");
        messages.remove("$schema");
        let messages = messages
            .into_iter()
            .map(|(k, v)| {
                let m = serde_json::from_value(v)
                    .unwrap_or_else(|e| panic!("message {k}: build.rs checked its shape: {e}"));
                (k, m)
            })
            .collect();
        Self { lang, messages }
    }

    /// The key's text, its `{name}`s filled from `args` (a number written in the language's
    /// way). A key the files lack (impossible: `k` is generated from them) says itself.
    pub fn t(&self, key: &str, args: &[(&str, Arg)]) -> String {
        let Some(message) = self.messages.get(key) else {
            return key.to_string();
        };
        let pattern = match message {
            Message::Plain(s) => s.as_str(),
            Message::Plural([v]) => {
                let count = args.iter().find_map(|(n, a)| match a {
                    Arg::Count(c) if *n == "count" => Some(*c),
                    _ => None,
                });
                let one = count.is_some_and(|c| self.is_one(c));
                v.variants
                    .get(if one { "p=one" } else { "p=*" })
                    .or_else(|| v.variants.get("p=*"))
                    .map_or(key, String::as_str)
            }
        };
        fill(pattern, |name| {
            args.iter().find(|(n, _)| *n == name).map(|(_, a)| match a {
                Arg::Text(s) => s.clone(),
                Arg::Count(c) => self.num(*c, 0),
            })
        })
    }

    /// The key's text without parameters.
    pub fn s(&self, key: &str) -> String {
        self.t(key, &[])
    }

    /// Intl.PluralRules' « one »: French 0 ≤ n < 2, English exactly 1.
    fn is_one(&self, n: f64) -> bool {
        match self.lang {
            Lang::Fr => (0.0..2.0).contains(&n),
            Lang::En => n == 1.0,
        }
    }

    /// `1 234,5` (a narrow no-break space between thousands, a comma) or `1,234.5`, with
    /// exactly `digits` decimals: the web app's `num` (Intl, minimum and maximum digits).
    pub fn num(&self, n: f64, digits: usize) -> String {
        let (group, point) = match self.lang {
            Lang::Fr => ('\u{202f}', ','),
            Lang::En => (',', '.'),
        };
        let fixed = format!("{:.*}", digits, n.abs());
        let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
        let mut out = String::new();
        // -0 is written 0
        if n < 0.0 && fixed.chars().any(|c| c.is_ascii_digit() && c != '0') {
            out.push('-');
        }
        for (i, c) in int.chars().enumerate() {
            if i > 0 && (int.len() - i) % 3 == 0 {
                out.push(group);
            }
            out.push(c);
        }
        if !frac.is_empty() {
            out.push(point);
            out.push_str(frac);
        }
        out
    }

    /// A time of day as Intl writes `{hour: 'numeric', minute: '2-digit'}`: « 7:05 », « 19:05 »;
    /// « 7:05 PM » in English (the web app's `clock`; the day periods typed from CLDR's en).
    pub fn clock(&self, t: NaiveTime) -> String {
        let (h, m) = (t.hour(), t.minute());
        match self.lang {
            Lang::Fr => format!("{h}:{m:02}"),
            Lang::En => {
                let (h12, half) = match h {
                    0 => (12, "AM"),
                    1..=11 => (h, "AM"),
                    12 => (12, "PM"),
                    _ => (h - 12, "PM"),
                };
                format!("{h12}:{m:02} {half}")
            }
        }
    }

    /// « 08:00 » as the backend writes a time of day, in the reader's format (`hhmm`).
    pub fn hhmm(&self, text: &str) -> String {
        match NaiveTime::parse_from_str(text, "%H:%M") {
            Ok(t) => self.clock(t),
            Err(_) => text.to_string(),
        }
    }

    /// A moment: its time today, else its day and time (« 12 nov., 19:05 »), the web's `when`.
    pub fn when<Tz: TimeZone>(&self, at: &DateTime<Tz>, now: &DateTime<Tz>) -> String {
        let clock = self.clock(at.time());
        if at.date_naive() == now.date_naive() {
            return clock;
        }
        let (d, m) = (at.day(), at.month0() as usize);
        match self.lang {
            Lang::Fr => format!("{d} {}, {clock}", MONTHS_FR[m]),
            Lang::En => format!("{} {d}, {clock}", MONTHS_EN[m]),
        }
    }

    /// « Aujourd'hui », « Demain », else the weekday (« samedi »): the web's `dayLabel`.
    pub fn day_label(&self, day: NaiveDate, today: NaiveDate) -> String {
        if day == today {
            return self.s(k::DAY_TODAY);
        }
        if today.succ_opt() == Some(day) {
            return self.s(k::DAY_TOMORROW);
        }
        let i = day.weekday().num_days_from_monday() as usize;
        match self.lang {
            Lang::Fr => WEEKDAYS_FR[i].to_string(),
            Lang::En => WEEKDAYS_EN[i].to_string(),
        }
    }

    /// « jeu. 12 nov. », « Thu, Nov 12 »: the web's `shortDay` (Intl's short weekday, day,
    /// short month).
    pub fn short_day(&self, day: NaiveDate) -> String {
        let (i, d, m) = (
            day.weekday().num_days_from_monday() as usize,
            day.day(),
            day.month0() as usize,
        );
        match self.lang {
            Lang::Fr => format!("{} {d} {}", WEEKDAYS_SHORT_FR[i], MONTHS_FR[m]),
            Lang::En => format!("{}, {} {d}", WEEKDAYS_SHORT_EN[i], MONTHS_EN[m]),
        }
    }

    /// « samedi 31 octobre », « samedi 1er novembre », « Saturday, October 31 »: the clock's date.
    pub fn long_date(&self, day: NaiveDate) -> String {
        let (i, d, m) = (
            day.weekday().num_days_from_monday() as usize,
            day.day(),
            day.month0() as usize,
        );
        match self.lang {
            Lang::Fr if d == 1 => format!("{} 1er {}", WEEKDAYS_FR[i], MONTHS_LONG_FR[m]),
            Lang::Fr => format!("{} {d} {}", WEEKDAYS_FR[i], MONTHS_LONG_FR[m]),
            Lang::En => format!("{}, {} {d}", WEEKDAYS_EN[i], MONTHS_LONG_EN[m]),
        }
    }

    /// « Novembre 2026 »: a month as the calendar's caption says it; `month0` from 0.
    pub fn month_year(&self, year: i32, month0: usize) -> String {
        let names = match self.lang {
            Lang::Fr => &MONTHS_LONG_FR,
            Lang::En => &MONTHS_LONG_EN,
        };
        let name = names[month0 % 12];
        let mut c = name.chars();
        let first: String = c.next().into_iter().flat_map(char::to_uppercase).collect();
        format!("{first}{} {year}", c.as_str())
    }

    /// « lun. », Monday first: a calendar's head.
    pub fn weekday_short(&self, i: usize) -> &'static str {
        match self.lang {
            Lang::Fr => WEEKDAYS_SHORT_FR[i % 7],
            Lang::En => WEEKDAYS_SHORT_EN[i % 7],
        }
    }

    /// A duration in seconds, two units at most (format.ts `formatDuration`).
    pub fn duration(&self, seconds: u64) -> String {
        const MINUTE: u64 = 60;
        const HOUR: u64 = 3600;
        const DAY: u64 = 86_400;
        let n = |v: u64| Arg::Count(v as f64);
        let t = seconds;
        if t < MINUTE {
            return self.t(k::DURATION_SECONDS, &[("s", n(t))]);
        }
        if t < HOUR {
            let (m, s) = (t / MINUTE, t % MINUTE);
            return if s == 0 {
                self.t(k::DURATION_MINUTES, &[("m", n(m))])
            } else {
                self.t(k::DURATION_MINUTES_SECONDS, &[("m", n(m)), ("s", n(s))])
            };
        }
        if t < DAY {
            let (h, m) = (t / HOUR, (t % HOUR) / MINUTE);
            return if m == 0 {
                self.t(k::DURATION_HOURS, &[("h", n(h))])
            } else {
                self.t(
                    k::DURATION_HOURS_MINUTES,
                    &[
                        ("h", n(h)),
                        ("m", n(m)),
                        ("mm", Arg::Text(format!("{m:02}"))),
                    ],
                )
            };
        }
        let (d, h) = (t / DAY, (t % DAY) / HOUR);
        if h == 0 {
            self.t(k::DURATION_DAYS, &[("d", n(d))])
        } else {
            self.t(k::DURATION_DAYS_HOURS, &[("d", n(d)), ("h", n(h))])
        }
    }

    /// « Injoignable depuis 12 min » within the hour, then since when it was last heard, or
    /// « Injoignable » when it never answered (format.ts `unreachableSince`).
    pub fn unreachable_since<Tz: TimeZone>(
        &self,
        at: Option<DateTime<Tz>>,
        now: &DateTime<Tz>,
    ) -> String {
        let Some(at) = at else {
            return self.s(k::STATE_UNREACHABLE);
        };
        let minutes = ((now.clone() - at.clone()).num_seconds() as f64 / 60.0)
            .round()
            .max(1.0) as u64;
        if minutes < 60 {
            self.t(
                k::STATE_UNREACHABLE_FOR,
                &[("duration", Arg::Text(self.duration(minutes * 60)))],
            )
        } else {
            self.t(
                k::STATE_UNREACHABLE_SINCE,
                &[("when", Arg::Text(self.when(&at, now)))],
            )
        }
    }

    /// A word inside a sentence (« bleu »), the web's `lower`.
    pub fn lower(&self, text: &str) -> String {
        text.to_lowercase()
    }

    /// « a, b et c », « a, b, and c » (Intl.ListFormat, long: CLDR's list patterns, typed).
    pub fn list(&self, items: &[String]) -> String {
        match (items, self.lang) {
            ([], _) => String::new(),
            ([one], _) => one.clone(),
            ([a, b], Lang::Fr) => format!("{a} et {b}"),
            ([a, b], Lang::En) => format!("{a} and {b}"),
            ([rest @ .., last], Lang::Fr) => format!("{} et {last}", rest.join(", ")),
            ([rest @ .., last], Lang::En) => format!("{}, and {last}", rest.join(", ")),
        }
    }

    /// What a failure means for the person (errors.ts `errorText`): a refusal by its code,
    /// else by its status; no answer as the network's.
    pub fn failure(&self, f: &crate::client::Failure) -> String {
        use crate::client::Failure;
        match f {
            Failure::Unreachable(_) => self.s(k::ERROR_NETWORK),
            Failure::SignedOut => self.s(&code_key(maison_api::codes::Code::NotSignedIn)),
            Failure::Refused { code: Some(c), .. } => self.s(&code_key(*c)),
            Failure::Refused { status: 403, .. } => self.s(k::CODE_FORBIDDEN),
            Failure::Refused { status: 404, .. } => self.s(k::CODE_NOT_FOUND),
            Failure::Refused {
                status: 502..=504, ..
            } => self.s(k::ERROR_NETWORK),
            Failure::Refused { .. } | Failure::Unreadable(_) => self.s(k::ERROR_SERVER),
        }
    }

    /// `80 %` (a narrow no-break space in French), from a 0..1 share: `percent(0.8)`.
    pub fn percent(&self, share: f64) -> String {
        let n = self.num(share * 100.0, 0);
        match self.lang {
            Lang::Fr => format!("{n}\u{202f}%"),
            Lang::En => format!("{n}%"),
        }
    }
}

// CLDR's names (fr, en), as Intl writes `month: 'short'`, `weekday: 'long'` and `'short'`
const MONTHS_FR: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
    "déc.",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_LONG_FR: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
const MONTHS_LONG_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS_FR: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];
const WEEKDAYS_EN: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const WEEKDAYS_SHORT_FR: [&str; 7] = ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."];
const WEEKDAYS_SHORT_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// The key of a refusal's words: `code_<code>` for every code (errors.ts `CODE_WORDS`, the same
/// convention; a test checks each one has its words).
fn code_key(code: maison_api::codes::Code) -> String {
    format!("code_{}", code.as_str())
}

/// `{name}` → its value; a name without one stays as written.
fn fill(pattern: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match value(name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `("count", Arg::Count(n))` in short.
pub fn count(n: usize) -> (&'static str, Arg) {
    ("count", Arg::Count(n as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_is_filled() {
        let fr = Words::new(Lang::Fr);
        let percent = fr.percent(0.8);
        assert_eq!(percent, "80\u{202f}%");
        assert_eq!(
            fr.t(k::LAMPS_ON_PERCENT, &[("percent", Arg::Count(80.0))]),
            "Allumée, 80\u{202f}%"
        );
        assert_eq!(Words::new(Lang::En).s(k::STATE_OFF), "Off");
    }

    #[test]
    fn plurals_follow_the_language() {
        let fr = Words::new(Lang::Fr);
        let en = Words::new(Lang::En);
        assert_eq!(fr.t(k::MEROSS_PLUG_COUNT, &[count(0)]), "0 prise");
        assert_eq!(fr.t(k::MEROSS_PLUG_COUNT, &[count(1)]), "1 prise");
        assert_eq!(fr.t(k::MEROSS_PLUG_COUNT, &[count(2)]), "2 prises");
        assert_eq!(en.t(k::MEROSS_PLUG_COUNT, &[count(0)]), "0 plugs");
        assert_eq!(en.t(k::MEROSS_PLUG_COUNT, &[count(1)]), "1 plug");
    }

    #[test]
    fn days_are_said_as_the_web_app_says_them() {
        let (fr, en) = (Words::new(Lang::Fr), Words::new(Lang::En));
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        // 12 November 2026 is a Thursday
        let today = d("2026-11-12");
        assert_eq!(fr.day_label(today, today), "Aujourd’hui");
        assert_eq!(fr.day_label(d("2026-11-13"), today), "Demain");
        assert_eq!(fr.day_label(d("2026-11-14"), today), "samedi");
        assert_eq!(en.day_label(d("2026-11-16"), today), "Monday");
        assert_eq!(fr.short_day(today), "jeu. 12 nov.");
        assert_eq!(en.short_day(today), "Thu, Nov 12");
        assert_eq!(fr.short_day(d("2027-02-01")), "lun. 1 févr.");
        assert_eq!(fr.long_date(today), "jeudi 12 novembre");
        assert_eq!(fr.long_date(d("2026-11-01")), "dimanche 1er novembre");
        assert_eq!(en.long_date(today), "Thursday, November 12");
        assert_eq!(fr.month_year(2026, 7), "Août 2026");
        assert_eq!(en.month_year(2027, 0), "January 2027");
        assert_eq!((fr.weekday_short(0), en.weekday_short(6)), ("lun.", "Sun"));
    }

    #[test]
    fn numbers_are_written_as_intl_writes_them() {
        let fr = Words::new(Lang::Fr);
        let en = Words::new(Lang::En);
        assert_eq!(fr.num(19.2345, 1), "19,2");
        assert_eq!(fr.num(19.0, 1), "19,0");
        assert_eq!(fr.num(1234.0, 0), "1\u{202f}234");
        assert_eq!(fr.num(12345.0, 0), "12\u{202f}345");
        assert_eq!(en.num(1234.5, 1), "1,234.5");
        assert_eq!(fr.num(-0.01, 1), "0,0");
        assert_eq!(fr.num(-2.5, 1), "-2,5");
        assert_eq!(en.percent(0.42), "42%");
    }

    #[test]
    fn every_message_reads_in_both_languages() {
        for lang in [Lang::Fr, Lang::En] {
            let w = Words::new(lang);
            assert!(w.messages.len() > 800);
        }
    }

    #[test]
    fn a_missing_parameter_stays_visible() {
        assert_eq!(
            fill("a {x} b {y}", |n| (n == "x").then(|| "1".into())),
            "a 1 b {y}"
        );
        assert_eq!(fill("open {", |_| None), "open {");
    }

    #[test]
    fn times_and_durations_as_the_web_app_says_them() {
        use chrono::TimeZone;
        let fr = Words::new(Lang::Fr);
        let en = Words::new(Lang::En);
        let t = NaiveTime::from_hms_opt(19, 5, 0).unwrap();
        assert_eq!(fr.clock(t), "19:05");
        assert_eq!(en.clock(t), "7:05 PM");
        assert_eq!(fr.hhmm("06:00"), "6:00");
        assert_eq!(en.hhmm("00:30"), "12:30 AM");
        let tz = maison_api::HOUSE_TZ;
        let now = tz.with_ymd_and_hms(2026, 11, 12, 20, 0, 0).unwrap();
        let earlier = tz.with_ymd_and_hms(2026, 11, 12, 19, 5, 0).unwrap();
        let yesterday = tz.with_ymd_and_hms(2026, 11, 11, 19, 5, 0).unwrap();
        assert_eq!(fr.when(&earlier, &now), "19:05");
        assert_eq!(fr.when(&yesterday, &now), "11 nov., 19:05");
        assert_eq!(en.when(&yesterday, &now), "Nov 11, 7:05 PM");
        assert_eq!(fr.duration(45), "45\u{a0}s");
        assert_eq!(fr.duration(200), "3\u{a0}min 20\u{a0}s");
        assert_eq!(fr.duration(5400), "1\u{a0}h\u{a0}30");
        assert_eq!(
            en.duration(5400),
            en.t(
                k::DURATION_HOURS_MINUTES,
                &[
                    ("h", Arg::Count(1.0)),
                    ("m", Arg::Count(30.0)),
                    ("mm", Arg::Text("30".into()))
                ]
            )
        );
        assert_eq!(fr.duration(2 * 86_400 + 4 * 3600), "2\u{a0}j 4\u{a0}h");
        assert_eq!(fr.unreachable_since(None, &now), "Injoignable");
        let ten = tz.with_ymd_and_hms(2026, 11, 12, 19, 50, 0).unwrap();
        assert_eq!(
            fr.unreachable_since(Some(ten), &now),
            "Injoignable depuis 10\u{a0}min"
        );
        assert_eq!(
            fr.unreachable_since(Some(yesterday), &now),
            "Injoignable depuis 11 nov., 19:05"
        );
        assert_eq!(fr.list(&["a".into(), "b".into(), "c".into()]), "a, b et c");
        assert_eq!(
            en.list(&["a".into(), "b".into(), "c".into()]),
            "a, b, and c"
        );
        assert_eq!(
            fr.failure(&crate::client::Failure::Unreachable("x".into())),
            fr.s(k::ERROR_NETWORK)
        );
    }

    #[test]
    fn a_refusal_is_said_by_its_code_else_its_status() {
        use crate::client::Failure;
        use maison_api::codes::Code;
        let fr = Words::new(Lang::Fr);
        let refused = |status, code| fr.failure(&Failure::Refused { status, code });
        assert_eq!(
            refused(404, Some(Code::PairingUnknown)),
            fr.s(k::CODE_PAIRING_UNKNOWN)
        );
        assert_eq!(
            refused(429, Some(Code::TooManyAttempts)),
            fr.s(k::CODE_TOO_MANY_ATTEMPTS)
        );
        assert_eq!(
            refused(503, Some(Code::DeviceUnreachable)),
            fr.s(k::CODE_DEVICE_UNREACHABLE)
        );
        assert_eq!(refused(409, Some(Code::Busy)), fr.s(k::CODE_BUSY));
        // without a code: its status
        assert_eq!(refused(403, None), fr.s(k::CODE_FORBIDDEN));
        assert_eq!(refused(404, None), fr.s(k::CODE_NOT_FOUND));
        assert_eq!(refused(502, None), fr.s(k::ERROR_NETWORK));
        // every code is said with words that exist
        for c in Code::ALL {
            assert!(!refused(c.status(), Some(*c)).is_empty());
        }
    }

    /// Every refusal the API answers has its words, in both languages.
    #[test]
    fn every_code_has_its_words() {
        for lang in [Lang::Fr, Lang::En] {
            let w = Words::new(lang);
            for c in maison_api::codes::Code::ALL {
                assert!(
                    w.messages.contains_key(&code_key(*c)),
                    "{:?}: no {}",
                    lang,
                    code_key(*c)
                );
            }
        }
    }

    #[test]
    fn languages() {
        assert_eq!(Lang::from_tag("en_GB.UTF-8"), Lang::En);
        assert_eq!(Lang::from_tag("fr"), Lang::Fr);
        assert_eq!(Lang::from_tag(""), Lang::Fr);
    }
}
