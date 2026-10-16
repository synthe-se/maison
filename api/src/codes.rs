//! Every refusal the API answers, by name (AGENTS.md: refusals are coded): its `code`, its HTTP
//! status and the backend's message for logs and curl. The web app words each code itself
//! (`web/src/lib/errors.ts`, typed by `web/src/lib/codes.ts`, generated from this table:
//! `cargo run --manifest-path api/Cargo.toml --bin shared-ts codes > web/src/lib/codes.ts`),
//! the wall frame too. 401 is « not signed in » alone: the apps read it as a session that ended.

macro_rules! codes {
    ($($(#[doc = $doc:literal])* $name:ident = ($code:literal, $status:literal, $message:literal);)*) => {
        /// A refusal, by name.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Code {
            $($(#[doc = $doc])* $name,)*
        }

        impl Code {
            /// Every code, in the table's order.
            pub const ALL: &[Code] = &[$(Code::$name),*];

            /// The name the apps read (`not_signed_in`).
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Code::$name => $code,)*
                }
            }

            /// Its HTTP status.
            pub const fn status(self) -> u16 {
                match self {
                    $(Code::$name => $status,)*
                }
            }

            /// The backend's words, for logs and curl (the apps say their own).
            pub const fn message(self) -> &'static str {
                match self {
                    $(Code::$name => $message,)*
                }
            }

            /// The code named `s`.
            pub fn parse(s: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|c| c.as_str() == s)
            }
        }
    };
}

codes! {
    /// No session, or it ended: the only 401.
    NotSignedIn = ("not_signed_in", 401, "Not signed in");
    /// An admin's.
    Forbidden = ("forbidden", 403, "Admin privileges required");
    /// A wall screen may not do this (a setting): someone signed in, or an app's token, may.
    NotForScreens = ("not_for_screens", 403, "A wall screen cannot change settings");
    BadName = ("bad_name", 400, "A name, 60 characters at most");
    /// A person's id that is not a slug (lowercase letters, digits, dashes).
    BadPerson = ("bad_person", 400, "A person's id: lowercase letters, digits and dashes");
    PasskeyOff = ("passkey_off", 503, "Passkeys need PUBLIC_URL (https)");
    InviteInvalid = ("invite_invalid", 404, "This invitation is no longer valid");
    CeremonyExpired = ("ceremony_expired", 400, "Too late: start again");
    PasskeyRejected = ("passkey_rejected", 400, "Passkey not accepted");
    UnknownPasskey = ("unknown_passkey", 404, "Unknown passkey");
    PasskeyExists = ("passkey_exists", 409, "Passkey already registered");
    LastPasskey = ("last_passkey", 409, "The last passkey stays");
    NotFound = ("not_found", 404, "Not found");
    /// This wants a passkey sign-in in the last minutes.
    ReauthNeeded = ("reauth_needed", 403, "Sign in with a passkey again first");
    /// An invitation by name for a name someone already has.
    PersonExists = ("person_exists", 409, "Someone already has this name");
    /// Removing oneself: another admin does it.
    NotYourself = ("not_yourself", 409, "Another admin removes you");
    /// No screen waits with this pairing code (mistyped, or its ten minutes are over).
    PairingUnknown = ("pairing_unknown", 404, "No screen waits with this code");
    /// A screen waits with this code, but on another network than the admin's.
    PairingElsewhere = ("pairing_elsewhere", 403, "Pair a screen from its own network");
    /// A screen, or an app's token, already has this name.
    ScreenExists = ("screen_exists", 409, "A screen already has this name");
    /// Too many tries: wait `Retry-After` seconds.
    TooManyAttempts = ("too_many_attempts", 429, "Too many attempts");
    /// A scene that would run another.
    NestedScene = ("nested_scene", 400, "A scene cannot run a scene");
    /// More actions than a scene holds.
    TooManyActions = ("too_many_actions", 400, "Too many actions");
    /// Not a device's address on the house's network (a private IPv4 or a `.local` name).
    NotADevice = ("not_a_device", 400, "Not a device address on the local network");
    /// A value the device or the API does not take (out of range, unknown, malformed).
    Invalid = ("invalid", 400, "Not a value this takes");
    /// The device is not set up yet (no address, no key, not paired).
    NotConfigured = ("not_configured", 409, "Not set up yet");
    /// The device is busy with another order of the same kind: one at a time.
    Busy = ("busy", 409, "Busy: one at a time");
    /// The device cannot do this.
    Unsupported = ("unsupported", 400, "Not supported by this device");
    /// The device did not answer (the detail is in the backend's log).
    DeviceUnreachable = ("device_unreachable", 503, "Device unreachable");
    /// A service out there did not answer (RTE, EDF, Open-Meteo).
    ServiceUnreachable = ("service_unreachable", 503, "Service unreachable");
    /// Our own failure: the detail is in the backend's log.
    Internal = ("internal", 500, "Internal server error");
}

/// The web app's module, as `web/src/lib/codes.ts` holds it.
pub fn typescript() -> String {
    let mut ts = String::from(
        "// Generated from api/src/codes.rs (`cargo run --manifest-path api/Cargo.toml --bin shared-ts codes >\n\
         // web/src/lib/codes.ts`): never edited. Every refusal the API answers, by name.\n\n\
         export const CODES = [\n",
    );
    let names: Vec<String> = Code::ALL
        .iter()
        .map(|c| format!("\t'{}'", c.as_str()))
        .collect();
    // oxfmt's style: no trailing comma
    ts.push_str(&names.join(",\n"));
    ts.push('\n');
    ts.push_str(
        "] as const;\n\n/** A refusal's name. */\nexport type Code = (typeof CODES)[number];\n",
    );
    ts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_is_unique_snake_case_and_parses_back() {
        let mut seen = std::collections::HashSet::new();
        for c in Code::ALL {
            let s = c.as_str();
            assert!(seen.insert(s), "{s} twice");
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{s}");
            assert_eq!(Code::parse(s), Some(*c));
            assert!((400..600).contains(&c.status()));
        }
        // 401 means « not signed in » alone
        assert!(
            Code::ALL
                .iter()
                .all(|c| (c.status() == 401) == (*c == Code::NotSignedIn))
        );
        assert_eq!(Code::parse("nope"), None);
    }

    /// The web app's module is this table's: regenerate it when a code changes.
    #[test]
    fn the_web_apps_codes_are_these() {
        assert_eq!(
            include_str!("../../web/src/lib/codes.ts"),
            typescript(),
            "web/src/lib/codes.ts is out of step: cargo run --manifest-path api/Cargo.toml --bin shared-ts codes > web/src/lib/codes.ts"
        );
    }
}
