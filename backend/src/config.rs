use std::{env, path::PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub frontend_dist_dir: PathBuf,
    pub auth_cookie_name: String,
    pub auth_cookie_secure: bool,
    /// Where Maison is reached (`https://home.example.com`): the passkeys' origin, its host
    /// their RP ID. `PUBLIC_URL`, else `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`.
    pub public_url: Option<String>,
    pub disable_bluetooth: bool,
    pub source_root: PathBuf,
    /// People, their passkeys, pending invitations (`people.rs`).
    pub auth_path: PathBuf,
    pub meross_devices_path: PathBuf,
    pub devices_path: PathBuf,
    pub device_cache_path: PathBuf,
    pub broadlink_codes_path: PathBuf,
    pub climate_state_path: PathBuf,
    pub refresh_tokens_path: PathBuf,
    pub hue_lamps_path: PathBuf,
    pub hue_blacklist_path: PathBuf,
    pub zigbee_lamps_path: PathBuf,
    pub zigbee_lamps_blacklist_path: PathBuf,
    pub nabaztag_config_path: PathBuf,
    pub nabaztag_host: Option<String>,
    pub ir_keymap_path: PathBuf,
    pub tv_config_path: PathBuf,
    pub androidtv_config_path: PathBuf,
    pub adb_key_path: PathBuf,
    pub atv_identity_path: PathBuf,
    pub ir_api_token: Option<String>,
    /// Whether an app's service token may come over the LAN's plain HTTP (`ALLOW_LAN_TOKENS`,
    /// off by default): the token would cross the Wi-Fi in clear. Tailscale (encrypted) and
    /// loopback are always allowed; the IR bridge's own token is not concerned.
    pub allow_lan_tokens: bool,
    /// A wall screen not seen for this many days is revoked (`SCREEN_IDLE_DAYS`, default 90;
    /// 0 keeps them forever): a frame thrown away or stolen stops opening the house.
    pub screen_idle_days: u32,
    pub matter_state_dir: PathBuf,
    pub matter_trust_dir: PathBuf,
    pub matter_test_roots: bool,
    /// Tempo's sources (`tempo/source.rs`): RTE's public open data, RTE's official API (used
    /// when `RTE_CLIENT_ID` and `RTE_CLIENT_SECRET` are set: a free data.rte-france.com
    /// account), api-couleur-tempo.fr when RTE fails, EDF's season (the quotas), data.gouv's
    /// prices, ODRE's éCO2mix and Open-Meteo (the forecast). Settings so the tests point them
    /// at a local stub.
    pub tempo_rte_url: String,
    pub tempo_rte_api_url: String,
    pub rte_client_id: Option<String>,
    pub rte_client_secret: Option<String>,
    pub tempo_fallback_url: String,
    pub tempo_edf_url: String,
    pub tempo_tarifs_url: String,
    pub tempo_odre_url: String,
    pub open_meteo_url: String,
    /// Tempo's files (`tempo/mod.rs`: the seasons, `model.json`, `netload.json`,
    /// `weather.json`), shared with `fit_tempo`.
    pub tempo_dir: PathBuf,
    /// Open-Meteo's geocoding (`sun::search`, the house's place by name): a setting so the
    /// tests point it at a local stub.
    pub geocoding_url: String,
    /// The scenes (`scenes.rs`).
    pub scenes_path: PathBuf,
    /// The Zigbee radio (`ZIGBEE_*`, `zigbee/config.rs`).
    pub zigbee: crate::zigbee::ZigbeeConfig,
}

/// The JWT secret a fresh checkout ships with; the backend refuses to start with it.
pub const DEFAULT_JWT_SECRET: &str = "super-secret-cat-key-change-me";

impl Config {
    /// The settings from the environment (`.env` included), defaults for the rest.
    pub fn from_env() -> Self {
        let source_root = env_text("MAISON_SOURCE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(default_source_root);
        Self::load(source_root, env_text)
    }

    /// Every setting at its default, every file under `source_root` (tests start from here).
    pub fn defaults(source_root: PathBuf) -> Self {
        Self::load(source_root, |_| None)
    }

    /// The one place each setting is named, with its default: `var` looks a name up.
    fn load(source_root: PathBuf, var: impl Fn(&str) -> Option<String>) -> Self {
        let path = |name: &str, default: &str| {
            var(name)
                .map(PathBuf::from)
                .unwrap_or_else(|| source_root.join(default))
        };
        // a service's address, without the trailing slash its paths are joined after
        let url = |name: &str, default: &str| {
            var(name)
                .unwrap_or_else(|| default.to_string())
                .trim_end_matches('/')
                .to_string()
        };
        let flag = |name: &str, default: bool| {
            var(name).map_or(default, |v| {
                if !is_truthy(&v)
                    && !matches!(
                        v.to_ascii_lowercase().as_str(),
                        "0" | "false" | "no" | "off"
                    )
                {
                    tracing::warn!(setting = name, value = %v, "not a yes or a no: read as no");
                }
                is_truthy(&v)
            })
        };

        // an address as kept: without its trailing slash, said once here

        Self {
            host: var("HOST").unwrap_or_else(|| "0.0.0.0".to_string()),
            port: parsed(&var, if var("PORT").is_some() { "PORT" } else { "API_PORT" }, 3033),
            jwt_secret: var("JWT_SECRET").unwrap_or_else(|| DEFAULT_JWT_SECRET.to_string()),
            frontend_dist_dir: path("FRONTEND_DIST_DIR", "web/build"),
            auth_cookie_name: var("AUTH_COOKIE_NAME").unwrap_or_else(|| "maison_session".to_string()),
            auth_cookie_secure: flag("AUTH_COOKIE_SECURE", true),
            public_url: var("PUBLIC_URL")
                .or_else(|| var("CLOUDFLARE_PUBLIC_HOSTNAME").map(|host| format!("https://{host}")))
                .map(|url| url.trim_end_matches('/').to_string()),
            disable_bluetooth: flag("DISABLE_BLUETOOTH", false),
            // A directory of its own, like matter/: the file is replaced by temp-file +
            // rename, so the service user must own the directory (the app dir is root's).
            auth_path: path("AUTH_JSON_PATH", "auth/auth.json"),
            meross_devices_path: path("MEROSS_DEVICES_JSON_PATH", "meross-devices.json"),
            devices_path: path("DEVICES_JSON_PATH", "devices.json"),
            device_cache_path: path("DEVICE_CACHE_JSON_PATH", "device-cache.json"),
            broadlink_codes_path: path("BROADLINK_CODES_JSON_PATH", "broadlink-codes.json"),
            climate_state_path: path("CLIMATE_STATE_JSON_PATH", "climate-state.json"),
            refresh_tokens_path: path("REFRESH_TOKENS_JSON_PATH", "refresh-tokens.json"),
            hue_lamps_path: path("HUE_LAMPS_JSON_PATH", "hue-lamps.json"),
            hue_blacklist_path: path("HUE_BLACKLIST_JSON_PATH", "hue-lamps-blacklist.json"),
            zigbee_lamps_path: path("ZIGBEE_LAMPS_JSON_PATH", "zigbee-lamps.json"),
            zigbee_lamps_blacklist_path: path(
                "ZIGBEE_LAMPS_BLACKLIST_JSON_PATH",
                "zigbee-lamps-blacklist.json",
            ),
            nabaztag_config_path: path("NABAZTAG_JSON_PATH", "nabaztag.json"),
            nabaztag_host: var("NABAZTAG_HOST"),
            ir_keymap_path: path("IR_KEYMAP_JSON_PATH", "ir-keymap.json"),
            tv_config_path: path("TV_JSON_PATH", "tv.json"),
            androidtv_config_path: path("ANDROIDTV_JSON_PATH", "androidtv.json"),
            // Private key authenticating this host to the box's adbd.
            adb_key_path: path("ADB_KEY_PATH", "adb-key"),
            // TLS client identity for the Android TV Remote v2 protocol. Distinct from the
            // ADB key: the TV pairs against this certificate.
            atv_identity_path: path("ATV_IDENTITY_PATH", "atv-identity"),
            // Machine token for the STB IR bridge (kird); unset = IR API off.
            ir_api_token: var("IR_API_TOKEN"),
            allow_lan_tokens: flag("ALLOW_LAN_TOKENS", false),
            screen_idle_days: parsed(&var, "SCREEN_IDLE_DAYS", 90),
            // Matter fabric keys + commissioned covers. A directory, not a file: the
            // controller saves by temp-file + rename, so the service user must own the
            // directory itself.
            matter_state_dir: path("MATTER_STATE_DIR", "matter"),
            // Device-attestation roots (paa/ + cd/), refreshed by scripts/update-matter-trust.sh.
            matter_trust_dir: path("MATTER_TRUST_DIR", "matter-trust"),
            // Development only: accept the CSA test roots (matter.js / chip example devices)
            // instead of the production ones.
            matter_test_roots: flag("MATTER_TEST_ROOTS", false),
            tempo_rte_url: url("TEMPO_RTE_URL", "https://www.services-rte.com/cms/open_data/v1"),
            tempo_rte_api_url: url("TEMPO_RTE_API_URL", "https://digital.iservices.rte-france.com"),
            rte_client_id: var("RTE_CLIENT_ID"),
            rte_client_secret: var("RTE_CLIENT_SECRET"),
            tempo_fallback_url: url("TEMPO_FALLBACK_URL", "https://www.api-couleur-tempo.fr"),
            tempo_edf_url: url("TEMPO_EDF_URL", "https://api-commerce.edf.fr/commerce-activet/api/v1"),
            // a whole request (its query included): taken as it is
            tempo_tarifs_url: var("TEMPO_TARIFS_URL").unwrap_or_else(|| {
                "https://tabular-api.data.gouv.fr/api/resources/0c3d1d36-c412-4620-8566-e5cbb4fa2b5a/data/?page_size=1&P_SOUSCRITE__exact=6&__id__sort=desc".to_string()
            }),
            tempo_odre_url: url("TEMPO_ODRE_URL", "https://odre.opendatasoft.com/api/explore/v2.1/catalog/datasets"),
            open_meteo_url: url("OPEN_METEO_URL", "https://api.open-meteo.com"),
            tempo_dir: path("TEMPO_CACHE_DIR", "cache/tempo"),
            geocoding_url: url("GEOCODING_URL", "https://geocoding-api.open-meteo.com"),
            scenes_path: path("SCENES_JSON_PATH", "scenes.json"),
            zigbee: crate::zigbee::ZigbeeConfig::load(&var),
            source_root,
        }
    }

    /// The address to listen on.
    pub fn listen_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// An environment variable, trimmed; unset and empty are the same: absent.
pub fn env_text(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// `1`, `true`, `yes`, `on` (any case) are true; anything else is false.
fn is_truthy(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// The setting `name` parsed, else `default`; a value that does not parse is said in the
/// log (a typo must not pass for the default without a word).
fn parsed<T: std::str::FromStr>(
    var: &impl Fn(&str) -> Option<String>,
    name: &str,
    default: T,
) -> T {
    let Some(value) = var(name) else {
        return default;
    };
    value.parse().unwrap_or_else(|_| {
        tracing::warn!(setting = name, %value, "unparsable setting: using its default");
        default
    })
}

fn default_source_root() -> PathBuf {
    if let Ok(current_dir) = env::current_dir() {
        if looks_like_source_root(&current_dir) {
            return current_dir;
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        for ancestor in current_exe.ancestors() {
            if looks_like_source_root(ancestor) {
                return ancestor.to_path_buf();
            }
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("backend has parent")
        .to_path_buf()
}

fn looks_like_source_root(path: &std::path::Path) -> bool {
    path.join("devices.json").is_file() || path.join("web").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(pairs: &'static [(&'static str, &'static str)]) -> Config {
        Config::load(PathBuf::from("/x"), |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.to_string())
        })
    }

    #[test]
    fn addresses_lose_their_trailing_slash_once_here() {
        let c = with(&[
            ("PUBLIC_URL", "https://home.example.com/"),
            ("OPEN_METEO_URL", "http://127.0.0.1:9/"),
        ]);
        assert_eq!(c.public_url.as_deref(), Some("https://home.example.com"));
        assert_eq!(c.open_meteo_url, "http://127.0.0.1:9");
        assert_eq!(
            with(&[("CLOUDFLARE_PUBLIC_HOSTNAME", "home.example.com")])
                .public_url
                .as_deref(),
            Some("https://home.example.com")
        );
    }

    #[test]
    fn lan_tokens_are_off_and_screens_go_after_ninety_idle_days_by_default() {
        let c = with(&[]);
        assert!(!c.allow_lan_tokens);
        assert_eq!(c.screen_idle_days, 90);
        let c = with(&[("ALLOW_LAN_TOKENS", "yes"), ("SCREEN_IDLE_DAYS", "0")]);
        assert!(c.allow_lan_tokens);
        assert_eq!(c.screen_idle_days, 0);
    }

    #[test]
    fn numbers_parse_or_fall_back_to_their_default() {
        assert_eq!(with(&[]).port, 3033);
        assert_eq!(with(&[("PORT", "8080")]).port, 8080);
        assert_eq!(with(&[("API_PORT", "8081")]).port, 8081);
        assert_eq!(
            with(&[("PORT", "8080"), ("API_PORT", "8081")]).port,
            8080,
            "PORT first"
        );
        assert_eq!(
            with(&[("PORT", "eighty")]).port,
            3033,
            "garbage: the default (and a warning)"
        );
        assert_eq!(
            with(&[("ZIGBEE_PERMIT_JOIN_SECONDS", "-1")])
                .zigbee
                .permit_join_seconds,
            120,
            "out of range: the default"
        );
        assert_eq!(
            with(&[("ZIGBEE_PERMIT_JOIN_SECONDS", "60")])
                .zigbee
                .permit_join_seconds,
            60
        );
    }

    #[test]
    fn a_service_address_loses_its_trailing_slash_once_here() {
        let config = with(&[
            ("TEMPO_EDF_URL", "http://edf.example/api/"),
            ("GEOCODING_URL", "http://geo.example//"),
        ]);
        assert_eq!(config.tempo_edf_url, "http://edf.example/api");
        assert_eq!(config.geocoding_url, "http://geo.example");
        assert_eq!(config.open_meteo_url, "https://api.open-meteo.com");
        assert_eq!(config.tempo_dir, PathBuf::from("/x/cache/tempo"));
    }
}
