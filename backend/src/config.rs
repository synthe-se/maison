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
    pub zigbee_permit_join_seconds: u16,
    pub ir_keymap_path: PathBuf,
    pub tv_config_path: PathBuf,
    pub androidtv_config_path: PathBuf,
    pub adb_key_path: PathBuf,
    pub atv_identity_path: PathBuf,
    pub ir_api_token: Option<String>,
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
        let flag = |name: &str, default: bool| var(name).map(|v| is_truthy(&v)).unwrap_or(default);

        Self {
            host: var("HOST").unwrap_or_else(|| "0.0.0.0".to_string()),
            port: var("PORT")
                .or_else(|| var("API_PORT"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(3033),
            jwt_secret: var("JWT_SECRET").unwrap_or_else(|| DEFAULT_JWT_SECRET.to_string()),
            frontend_dist_dir: path("FRONTEND_DIST_DIR", "web/build"),
            auth_cookie_name: var("AUTH_COOKIE_NAME").unwrap_or_else(|| "maison_session".to_string()),
            auth_cookie_secure: flag("AUTH_COOKIE_SECURE", true),
            public_url: var("PUBLIC_URL")
                .or_else(|| var("CLOUDFLARE_PUBLIC_HOSTNAME").map(|host| format!("https://{host}"))),
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
            zigbee_permit_join_seconds: parsed(&var, "ZIGBEE_PERMIT_JOIN_SECONDS", 120),
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
            // Matter fabric keys + commissioned covers. A directory, not a file: the
            // controller saves by temp-file + rename, so the service user must own the
            // directory itself.
            matter_state_dir: path("MATTER_STATE_DIR", "matter"),
            // Device-attestation roots (paa/ + cd/), refreshed by scripts/update-matter-trust.sh.
            matter_trust_dir: path("MATTER_TRUST_DIR", "matter-trust"),
            // Development only: accept the CSA test roots (matter.js / chip example devices)
            // instead of the production ones.
            matter_test_roots: flag("MATTER_TEST_ROOTS", false),
            tempo_rte_url: var("TEMPO_RTE_URL")
                .unwrap_or_else(|| "https://www.services-rte.com/cms/open_data/v1".to_string()),
            tempo_rte_api_url: var("TEMPO_RTE_API_URL")
                .unwrap_or_else(|| "https://digital.iservices.rte-france.com".to_string()),
            rte_client_id: var("RTE_CLIENT_ID"),
            rte_client_secret: var("RTE_CLIENT_SECRET"),
            tempo_fallback_url: var("TEMPO_FALLBACK_URL")
                .unwrap_or_else(|| "https://www.api-couleur-tempo.fr".to_string()),
            tempo_edf_url: var("TEMPO_EDF_URL")
                .unwrap_or_else(|| "https://api-commerce.edf.fr/commerce-activet/api/v1".to_string()),
            tempo_tarifs_url: var("TEMPO_TARIFS_URL").unwrap_or_else(|| {
                "https://tabular-api.data.gouv.fr/api/resources/0c3d1d36-c412-4620-8566-e5cbb4fa2b5a/data/?page_size=1&P_SOUSCRITE__exact=6&__id__sort=desc".to_string()
            }),
            tempo_odre_url: var("TEMPO_ODRE_URL")
                .unwrap_or_else(|| "https://odre.opendatasoft.com/api/explore/v2.1/catalog/datasets".to_string()),
            open_meteo_url: var("OPEN_METEO_URL").unwrap_or_else(|| "https://api.open-meteo.com".to_string()),
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
pub fn is_truthy(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn parsed<T: std::str::FromStr>(var: &impl Fn(&str) -> Option<String>, name: &str, default: T) -> T {
    var(name).and_then(|v| v.parse().ok()).unwrap_or(default)
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
