pub mod adb;
pub mod androidtv;
pub mod atvremote;
pub mod auth;
pub mod broadlink;
pub mod broadlink_ir;
pub mod config;
pub mod error;
#[cfg(feature = "bluetooth")]
pub mod hue;
#[cfg(not(feature = "bluetooth"))]
#[path = "hue_stub.rs"]
pub mod hue;
pub mod ir;
pub mod json_config;
pub mod lamps;
pub mod matter;
pub mod meross;
pub mod mitsubishi_ir;
pub mod nabaztag;
pub mod net;
pub mod passkey;
pub mod people;
pub mod philips_ir;
pub mod routes;
pub mod scenes;
pub mod store;
pub mod sun;
pub mod tempo;
pub mod tuya;
pub mod tv;
pub mod util;
pub mod zigbee;

pub use tempo::TempoService;

use std::sync::Arc;

use axum::Router;
use axum::middleware::{self, Next};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use auth::RefreshTokenStore;
use broadlink::BroadlinkManager;
use config::Config;
use error::AppError;
use hue::HueManager;
use ir::IrManager;
use matter::MatterManager;
use nabaztag::NabaztagManager;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::services::{ServeDir, ServeFile};
use passkey::PasskeyState;
use scenes::ScenesManager;
use people::People;
use meross::MerossManager;
use tuya::TuyaManager;
use tv::TvManager;
use androidtv::AndroidTvManager;
use tower_http::trace::TraceLayer;
use zigbee::ZigbeeManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub(crate) people: Arc<People>,
    /// `None` without a usable `PUBLIC_URL`: nobody can sign in (said in the log).
    pub(crate) passkeys: Option<Arc<PasskeyState>>,
    pub(crate) refresh_store: RefreshTokenStore,
    pub(crate) broadlink: BroadlinkManager,
    pub(crate) hue: HueManager,
    pub(crate) ir: IrManager,
    pub(crate) scenes: ScenesManager,
    pub(crate) matter: MatterManager,
    pub(crate) meross: MerossManager,
    pub(crate) nabaztag: NabaztagManager,
    pub(crate) tempo: TempoService,
    pub(crate) tuya: TuyaManager,
    pub(crate) tv: TvManager,
    pub(crate) androidtv: AndroidTvManager,
    pub(crate) zigbee: ZigbeeManager,
}

/// The app from the environment (`.env` included): the server's.
pub fn app_parts_from_env() -> Result<(Router, AppState), AppError> {
    build_app_parts_from_config(Arc::new(Config::from_env()))
}

/// The app and its state from `config`: the routes, and the background jobs started.
pub fn build_app_parts_from_config(config: Arc<Config>) -> Result<(Router, AppState), AppError> {
    let people = Arc::new(People::new(&config.auth_path));
    let passkeys = PasskeyState::new(&config).map(Arc::new);
    let refresh_store = RefreshTokenStore::load(&config.refresh_tokens_path);
    let broadlink =
        BroadlinkManager::new(&config.broadlink_codes_path, &config.climate_state_path)?;
    let hue = HueManager::new(config.as_ref())?;
    let ir = IrManager::new(&config.ir_keymap_path)?;
    let scenes = ScenesManager::new(&config.scenes_path)?;
    let matter = MatterManager::new(config.as_ref())?;
    let meross = MerossManager::new(&config.meross_devices_path)?;
    let nabaztag = NabaztagManager::new(
        &config.nabaztag_config_path,
        config.nabaztag_host.as_deref(),
    )?;
    let tempo = TempoService::from_config(&config)?;
    let tuya = TuyaManager::new(&config.devices_path, &config.device_cache_path)?;
    let tv = TvManager::new(&config.tv_config_path, broadlink.clone())?;
    let androidtv =
        AndroidTvManager::new(
        &config.androidtv_config_path,
        &config.adb_key_path,
        &config.atv_identity_path,
    )?;
    let zigbee = ZigbeeManager::new(config.as_ref())?;

    let state = AppState {
        config,
        people,
        passkeys,
        refresh_store,
        broadlink,
        hue,
        ir,
        scenes,
        matter,
        meross,
        nabaztag,
        tempo,
        tuya,
        tv,
        androidtv,
        zigbee,
    };

    let startup_tuya = state.tuya.clone();
    tokio::spawn(async move { startup_tuya.connect_all_devices().await });

    // The shutters' sun schedule: a look every 30 s sends what fell due since the last one.
    let schedule = state.matter.clone();
    every("sun schedule", std::time::Duration::from_secs(30), move || {
        let schedule = schedule.clone();
        async move { schedule.run_schedule().await }
    });

    // Mirror the daily Tempo colors on the Nabaztag. The push is two UDP datagrams and
    // idempotent, so it simply repeats every 15 minutes: that also re-applies the colors
    // after a rabbit reboot.
    let tempo_rabbit = state.clone();
    every("tempo on the rabbit", std::time::Duration::from_secs(900), move || {
        let state = tempo_rabbit.clone();
        async move {
            let config = state.nabaztag.config().await;
            if !config.tempo_enabled || config.host.is_none() {
                return;
            }
            if let Err(error) = state.nabaztag.push_tempo_from(&state.tempo, false).await {
                tracing::debug!(%error, "tempo push to the rabbit failed");
            }
        }
    });

    let app = build_app(state.clone());
    Ok((app, state))
}

/// A background task that logs a panic instead of dying silently; aborting the handle
/// stops it.
// only the Bluetooth (Hue) loops use it today
#[cfg_attr(not(feature = "bluetooth"), allow(dead_code))]
pub(crate) fn supervised(name: &'static str, task: impl std::future::Future<Output = ()> + Send + 'static) -> tokio::task::JoinHandle<()> {
    use futures::FutureExt;
    tokio::spawn(async move {
        if std::panic::AssertUnwindSafe(task).catch_unwind().await.is_err() {
            tracing::error!(task = name, "background task panicked");
        }
    })
}

/// A background job run every `period` until its handle is aborted (or for the life of the
/// process): a panic in one run is logged and the next run happens anyway (a bare loop would
/// die silently). Aborting the handle also stops a run in flight.
pub(crate) fn every<F, Fut>(name: &'static str, period: std::time::Duration, job: F) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    use futures::FutureExt;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(period);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if std::panic::AssertUnwindSafe(job()).catch_unwind().await.is_err() {
                tracing::error!(job = name, "background job panicked; it runs again next time");
            }
        }
    })
}

pub fn build_app(state: AppState) -> Router {
    // Every device and settings route needs someone signed in, said once here: a handler
    // never has to remember it. Admin-only ones add `AdminUser`. Signing in itself
    // (`auth`, `passkeys`), `/health` and the IR bridge's machine route (`ir`, whose
    // handlers name their own extractor) stay outside.
    let signed_in = Router::<AppState>::new()
        .nest("/broadlink", routes::broadlink::router())
        .nest("/devices", routes::devices::router())
        .nest("/hue-lamps", routes::hue::router())
        .nest("/matter", routes::matter::router())
        .nest("/meross", routes::meross::router())
        .nest("/nabaztag", routes::nabaztag::router())
        .nest("/scenes", routes::scenes::router())
        .nest("/tempo", routes::tempo::router())
        .nest("/tv", routes::tv::router())
        .nest("/androidtv", routes::androidtv::router())
        .nest("/zigbee", routes::zigbee::router())
        .route_layer(middleware::from_extractor_with_state::<auth::AuthenticatedUser, AppState>(state.clone()));
    let api_router = Router::<AppState>::new()
        .merge(routes::root::api_router())
        .nest("/auth", routes::auth::router())
        .merge(routes::passkeys::router())
        .nest("/ir", routes::ir::router())
        .merge(signed_in);

    let app = Router::<AppState>::new()
        .merge(routes::root::health_router())
        .nest("/api", api_router);

    let csp = content_security_policy(&state.config.frontend_dist_dir);
    let app = if state.config.frontend_dist_dir.join("index.html").is_file() {
        app.fallback_service(
            ServeDir::new(state.config.frontend_dist_dir.clone())
                .not_found_service(ServeFile::new(state.config.frontend_dist_dir.join("index.html"))),
        )
    } else {
        app.merge(routes::root::router())
    };

    // CORS: reject all cross-origin requests. The frontend is served from the
    // same origin so legitimate requests never need CORS.
    app.layer(middleware::from_fn(move |request, next| {
        security_headers(request, next, csp.clone())
    }))
        .layer(middleware::from_fn(guard_request))
        .layer(CorsLayer::new())
        // Last-resort guard so no request can hang a connection forever; the
        // limit sits above every legitimate long operation (IR learning,
        // device discovery, Zigbee command reply timeout of 120s).
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            std::time::Duration::from_secs(180),
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::extract::Request| {
                    tracing::info_span!(
                        "http",
                        method = %request.method(),
                        uri = %loggable_path(request.uri().path()),
                    )
                }),
        )
        .with_state(state)
}

/// What a request may do before any route sees it (`request_allowed`).
async fn guard_request(request: axum::extract::Request, next: Next) -> Response {
    use axum::extract::{ConnectInfo, connect_info::MockConnectInfo};
    // the server's peer address, or the one a test mocks (as axum's ConnectInfo reads it)
    let extensions = request.extensions();
    let peer = extensions
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0)
        .or_else(|| extensions.get::<MockConnectInfo<std::net::SocketAddr>>().map(|m| m.0))
        .map(|addr| addr.ip());
    match request_allowed(peer, request.method(), request.uri().path(), request.headers()) {
        Ok(()) => next.run(request).await,
        Err(why) => {
            tracing::warn!(?peer, path = %loggable_path(request.uri().path()), why, "request refused");
            AppError::forbidden(why).into_response()
        }
    }
}

/// Two rules, before routing:
/// - From the LAN (not loopback, where cloudflared and the dev proxy are), only `/health`
///   and the IR bridge's `/api/ir/key`: everything else goes through the tunnel's HTTPS.
///   Passkeys cannot sign in over the LAN address anyway; this also keeps the API and its
///   cookies off cleartext HTTP.
/// - A request that changes something must come from this site: a browser says where it
///   comes from (`Sec-Fetch-Site`, else `Origin` against `Host`); a sibling subdomain is
///   « same-site » but not this site. Clients that say nothing (kird, curl) pass.
fn request_allowed(
    peer: Option<std::net::IpAddr>,
    method: &axum::http::Method,
    path: &str,
    headers: &axum::http::HeaderMap,
) -> Result<(), &'static str> {
    let local = peer.is_none_or(|ip| ip.to_canonical().is_loopback());
    if !local && !matches!(path, "/health" | "/api/ir/key") {
        return Err("Use the public address");
    }
    if matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS) {
        return Ok(());
    }
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    match header("sec-fetch-site") {
        Some("same-origin" | "none") => Ok(()),
        Some(_) => Err("Cross-site request refused"),
        None => match (header("origin"), header("host")) {
            (Some(origin), Some(host)) if origin.split_once("://").map(|(_, h)| h) != Some(host) => {
                Err("Cross-origin request refused")
            }
            _ => Ok(()),
        },
    }
}

/// How long a response may be kept, by Cloudflare's edge and by the browser. Said for
/// every path: without it Cloudflare keeps static files 4 h, and a new logo or icon
/// stays invisible that long after a deploy.
/// - `/api/`: answers about the house and its people, never kept;
/// - `/_app/immutable/`: SvelteKit's hashed build files, a new name for every change: a year;
/// - everything else (the page, the logo, icons, the manifest): kept but revalidated each time.
fn cache_policy(path: &str) -> &'static str {
    if path.starts_with("/api/") {
        "no-store"
    } else if path.starts_with("/_app/immutable/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

/// A path as logs keep it: an invitation's token is a secret (the query string, a town
/// searched for, is left out altogether).
fn loggable_path(path: &str) -> std::borrow::Cow<'_, str> {
    match path.strip_prefix("/api/invites/") {
        Some(rest) if rest.len() > 16 => "/api/invites/…".into(),
        _ => path.into(),
    }
}

async fn security_headers(
    request: axum::extract::Request,
    next: Next,
    csp: header::HeaderValue,
) -> Response {
    let cache = cache_policy(request.uri().path());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, header::HeaderValue::from_static(cache));
    let fixed = header::HeaderValue::from_static;
    headers.insert("cross-origin-opener-policy", fixed("same-origin"));
    headers.insert("permissions-policy", fixed("camera=(), microphone=(), geolocation=(), payment=(), usb=()"));
    headers.insert(header::X_FRAME_OPTIONS, fixed("DENY"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, fixed("nosniff"));
    headers.insert(header::STRICT_TRANSPORT_SECURITY, fixed("max-age=63072000; includeSubDomains"));
    headers.insert(header::CONTENT_SECURITY_POLICY, csp);
    headers.insert(header::REFERRER_POLICY, fixed("strict-origin-when-cross-origin"));
    response
}

/// The page's Content-Security-Policy. Scripts: our own files, plus exactly the inline
/// scripts of the built `index.html` (SvelteKit's boot script, the theme set before first
/// paint), allowed by their SHA-256 — never `'unsafe-inline'`. Styles keep `'unsafe-inline'`:
/// Svelte's `style:` directives write inline styles (a progress bar's width, a colour swatch).
fn content_security_policy(dist_dir: &std::path::Path) -> header::HeaderValue {
    let page = std::fs::read_to_string(dist_dir.join("index.html")).unwrap_or_default();
    let hashes: String = inline_scripts(&page)
        .map(|script| {
            use base64::Engine;
            use sha2::Digest;
            let digest = sha2::Sha256::digest(script.as_bytes());
            format!(" 'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest))
        })
        .collect();
    format!(
        "default-src 'self'; script-src 'self'{hashes}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'; object-src 'none'"
    )
    .parse()
    .expect("a CSP made of ASCII")
}

/// The bodies of `<script>` elements without a `src`, as the browser hashes them.
fn inline_scripts(page: &str) -> impl Iterator<Item = &str> {
    page.split("<script").skip(1).filter_map(|rest| {
        let (attrs, after) = rest.split_once('>')?;
        let (body, _) = after.split_once("</script>")?;
        (!attrs.contains("src=")).then_some(body)
    })
}

impl AppState {
    pub fn validate_runtime_security(&self) -> Result<(), AppError> {
        jwt_secret_problem(&self.config.jwt_secret).map_or(Ok(()), |why| {
            Err(AppError::http(axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("Refusing to start: {why}")))
        })
    }

    /// Before the process ends: the radios and the device sessions are closed cleanly.
    pub async fn shutdown(&self) {
        self.hue.shutdown().await;
        self.zigbee.shutdown().await;
        self.tuya.disconnect_all_devices().await;
    }
}

/// Why `secret` cannot sign sessions: anyone who guesses it forges an admin's token from
/// the internet. At least 32 bytes, and none of the placeholders the examples ship.
fn jwt_secret_problem(secret: &str) -> Option<&'static str> {
    const PLACEHOLDERS: &[&str] = &[config::DEFAULT_JWT_SECRET, "change-me", "changeme", "secret"];
    if PLACEHOLDERS.iter().any(|p| secret.eq_ignore_ascii_case(p)) {
        Some("JWT_SECRET is a placeholder; set a random one in .env (openssl rand -hex 32)")
    } else if secret.len() < 32 {
        Some("JWT_SECRET is shorter than 32 bytes; set a random one in .env (openssl rand -hex 32)")
    } else {
        None
    }
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use axum::http::{HeaderMap, Method};

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn the_lan_reaches_only_health_and_the_ir_bridge() {
        let lan = Some("192.168.1.20".parse().unwrap());
        let tunnel = Some("127.0.0.1".parse().unwrap());
        let none = HeaderMap::new();
        assert!(request_allowed(lan, &Method::POST, "/api/ir/key", &none).is_ok());
        assert!(request_allowed(lan, &Method::GET, "/health", &none).is_ok());
        assert!(request_allowed(lan, &Method::GET, "/api/meross", &none).is_err());
        assert!(request_allowed(lan, &Method::GET, "/", &none).is_err());
        assert!(request_allowed(tunnel, &Method::GET, "/api/meross", &none).is_ok());
        assert!(request_allowed(Some("::ffff:127.0.0.1".parse().unwrap()), &Method::GET, "/x", &none).is_ok());
    }

    #[test]
    fn a_change_must_come_from_this_site() {
        let local = Some("127.0.0.1".parse().unwrap());
        let post = |h: &[(&'static str, &str)]| request_allowed(local, &Method::POST, "/api/meross/x/toggle", &headers(h));
        assert!(post(&[("sec-fetch-site", "same-origin")]).is_ok());
        assert!(post(&[("sec-fetch-site", "cross-site")]).is_err());
        assert!(post(&[("sec-fetch-site", "same-site")]).is_err(), "a sibling subdomain is not this site");
        assert!(post(&[("origin", "https://evil.example"), ("host", "home.kahn.studio")]).is_err());
        assert!(post(&[("origin", "https://home.kahn.studio"), ("host", "home.kahn.studio")]).is_ok());
        assert!(post(&[]).is_ok(), "kird and curl say nothing");
        assert!(request_allowed(local, &Method::GET, "/api/x", &headers(&[("sec-fetch-site", "cross-site")])).is_ok(), "reads are harmless");
    }

    #[test]
    fn every_path_says_how_long_it_may_be_kept() {
        assert_eq!(cache_policy("/api/tempo"), "no-store");
        assert_eq!(cache_policy("/_app/immutable/chunks/app.4f2a.js"), "public, max-age=31536000, immutable");
        assert_eq!(cache_policy("/brand.svg"), "no-cache", "a new logo shows at the next visit");
        assert_eq!(cache_policy("/"), "no-cache");
    }

    #[test]
    fn invitation_tokens_stay_out_of_the_logs() {
        assert_eq!(loggable_path(&format!("/api/invites/{}", "a".repeat(64))), "/api/invites/…");
        assert_eq!(loggable_path("/api/invites/0123456789abcdef"), "/api/invites/0123456789abcdef", "an invitation's id is not secret");
        assert_eq!(loggable_path("/api/meross"), "/api/meross");
    }

    #[test]
    fn a_guessable_secret_is_refused() {
        assert!(jwt_secret_problem("change-me").is_some());
        assert!(jwt_secret_problem(config::DEFAULT_JWT_SECRET).is_some());
        assert!(jwt_secret_problem("short-but-not-a-placeholder").is_some());
        assert!(jwt_secret_problem(&"x".repeat(32)).is_none());
    }
}

#[cfg(test)]
mod every_tests {
    use std::sync::{Arc, atomic::{AtomicU32, Ordering}};

    #[tokio::test(start_paused = true)]
    async fn a_panicking_run_is_followed_by_the_next_and_abort_stops_it() {
        let runs = Arc::new(AtomicU32::new(0));
        let counted = runs.clone();
        let handle = super::every("test", std::time::Duration::from_secs(1), move || {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            async move { assert!(n != 1, "the second run panics") }
        });
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 4, "runs at 0, 1 (panics), 2 and 3 s");
        handle.abort();
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        assert_eq!(runs.load(Ordering::SeqCst), 4, "nothing after the abort");
    }

    #[tokio::test]
    async fn a_supervised_panic_is_caught() {
        let task = super::supervised("test", async { panic!("boom") });
        assert!(task.await.is_ok(), "the panic is caught and logged");
    }
}

#[cfg(test)]
mod csp_tests {
    use super::*;

    #[test]
    fn inline_scripts_are_found_and_external_ones_skipped() {
        let page = r#"<head><script>a()</script><script type="module" src="/x.js"></script></head><body><script>
b()
</script></body>"#;
        let found: Vec<&str> = inline_scripts(page).collect();
        assert_eq!(found, vec!["a()", "\nb()\n"]);
    }

    #[test]
    fn the_policy_allows_exactly_the_inline_scripts() {
        let dir = crate::util::test_dir();
        // echo -n "a()" | openssl dgst -sha256 -binary | base64
        std::fs::write(dir.path().join("index.html"), "<script>a()</script>").unwrap();
        let csp = content_security_policy(dir.path());
        let csp = csp.to_str().unwrap();
        assert!(csp.contains("script-src 'self' 'sha256-qVpDBgj7bpq5hMAcGp3AOc79J3Y1Z4HvySTwKrWDoy4='"), "{csp}");
        let scripts = csp.split(';').find(|d| d.trim_start().starts_with("script-src")).unwrap();
        assert!(!scripts.contains("unsafe-inline"), "{scripts}");
    }

    #[test]
    fn without_a_built_frontend_only_our_own_scripts_run() {
        let csp = content_security_policy(std::path::Path::new("/nonexistent"));
        assert!(csp.to_str().unwrap().contains("script-src 'self';"));
    }
}
