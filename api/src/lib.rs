//! What Maison's HTTP API answers and reads, said once (AGENTS.md: one logic, one place): the
//! backend serializes these types, the wall frame's native interface (`frame/ui`) deserializes
//! them. JSON keys are camelCase English (`rename_all`), values keep their own spelling.

pub mod envelope;
pub mod lamps;
pub mod matter;
pub mod meross;
pub mod paces;
pub mod remote;
pub mod scenes;
pub mod screens;
pub mod sun;
pub mod tempo;

pub use envelope::{Answer, DeviceRef};

/// The house's time zone: a schedule « at sunset », a Tempo day (RTE's days are French
/// days). Said here and never as `chrono::Local`: the Pi has no tzdata, its Local is UTC.
pub const HOUSE_TZ: chrono_tz::Tz = chrono_tz::Europe::Paris;
