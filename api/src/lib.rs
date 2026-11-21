//! What Maison's HTTP API answers and reads, said once (AGENTS.md: one logic, one place): the
//! backend serializes these types, the wall frame's native interface (`frame/ui`) deserializes
//! them. JSON keys are camelCase English (`rename_all`), values keep their own spelling.

pub mod androidtv;
pub mod cats;
pub mod climate;
pub mod codes;
pub mod edge;
pub mod envelope;
pub mod events;
pub mod lamps;
pub mod matter;
pub mod meross;
pub mod nabaztag;
pub mod paces;
pub mod photos;
pub mod remote;
pub mod scenes;
pub mod screens;
pub mod sun;
pub mod tempo;
pub mod tv;

pub use envelope::{Answer, DeviceRef, Message, OfDevice};
/// The house's time zone's type (`HOUSE_TZ`'s): readers need no jiff of their own for it.
pub use jiff::tz::TimeZone as Tz;

/// The house's time zone: a schedule « at sunset », a Tempo day (RTE's days are French
/// days). Said here and never as the system's zone: a host without tzdata says UTC. From the
/// database jiff embeds, so the same on every host.
pub static HOUSE_TZ: std::sync::LazyLock<Tz> = std::sync::LazyLock::new(|| {
    Tz::get(HOUSE_TZ_NAME).expect("jiff embeds the time zone database (tzdb-bundle-always)")
});

/// `HOUSE_TZ`'s IANA name, for a service asked for its times in the house's zone (Open-Meteo).
pub const HOUSE_TZ_NAME: &str = "Europe/Paris";
