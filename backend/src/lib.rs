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
pub mod screens;
pub mod store;
pub mod sun;
pub mod tempo;
pub mod tuya;
pub mod tv;
pub mod util;
pub mod zigbee;

pub use tempo::TempoService;

use std::sync::Arc;

use androidtv::AndroidTvManager;
use auth::RefreshTokenStore;
use axum::http::header;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use broadlink::BroadlinkManager;
use config::Config;
use error::{AppError, Refusal};
use hue::HueManager;
use ir::IrManager;
use matter::MatterManager;
use meross::MerossManager;
use nabaztag::NabaztagManager;
use passkey::{Limiter, PasskeyState};
use people::People;
use scenes::ScenesManager;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tuya::TuyaManager;
use tv::TvManager;
use zigbee::ZigbeeManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub(crate) people: Arc<People>,
    /// `None` without a usable `PUBLIC_URL`: nobody can sign in (said in the log).
    pub(crate) passkeys: Option<Arc<PasskeyState>>,
    pub(crate) refresh_store: RefreshTokenStore,
    /// Failed machine authentications (the IR bridge's token, unknown service tokens), per
    /// address: past the limit, the address waits.
    pub(crate) auth_failures: Arc<Limiter>,
    /// Wall screens waiting for an admin to pair them.
    pub(crate) pairings: Arc<screens::Pairings>,
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
    let androidtv = AndroidTvManager::new(
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
        auth_failures: Arc::new(Limiter::new(
            AUTH_FAILURES_PER_MINUTE,
            std::time::Duration::from_secs(60),
        )),
        pairings: Arc::new(screens::Pairings::default()),
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

    // A screen paired but never back for its cookie leaves no token behind; a screen unseen
    // for `SCREEN_IDLE_DAYS` (thrown away, stolen) is revoked.
    let pairings = (
        state.pairings.clone(),
        state.people.clone(),
        i64::from(state.config.screen_idle_days) * 86_400_000,
    );
    every(
        "screen tokens",
        std::time::Duration::from_secs(60),
        move || {
            let (pairings, people, idle_ms) = pairings.clone();
            async move {
                for name in pairings.expired_grants() {
                    if let Err(error) = people.revoke_token(&name).await {
                        tracing::warn!(%error, screen = %name, "an unclaimed screen token was not revoked");
                    }
                }
                if idle_ms > 0 {
                    match people.revoke_idle_screens(util::now_ms(), idle_ms).await {
                        Ok(names) => {
                            for name in names {
                                tracing::warn!(screen = %name, "a wall screen unseen for too long: revoked");
                            }
                        }
                        Err(error) => tracing::warn!(%error, "idle wall screens not swept"),
                    }
                }
            }
        },
    );

    // The shutters' sun schedule: a look every 30 s sends what fell due since the last one.
    let schedule = state.matter.clone();
    every(
        "sun schedule",
        std::time::Duration::from_secs(30),
        move || {
            let schedule = schedule.clone();
            async move { schedule.run_schedule().await }
        },
    );

    // Mirror the daily Tempo colors on the Nabaztag: pushed when Tempo's answer changes,
    // when the day turns, and every hour (a rabbit reboot); restarted should it end.
    let tempo_rabbit = state.clone();
    every(
        "tempo on the rabbit",
        nabaztag::TEMPO_SAFETY_NET,
        move || {
            let state = tempo_rabbit.clone();
            async move { state.nabaztag.follow_tempo(&state.tempo).await }
        },
    );

    let app = build_app(state.clone());
    Ok((app, state))
}

/// A background task that logs a panic instead of dying silently; aborting the handle
/// stops it.
// only the Bluetooth (Hue) loops use it today
#[cfg_attr(not(feature = "bluetooth"), allow(dead_code))]
pub(crate) fn supervised(
    name: &'static str,
    task: impl std::future::Future<Output = ()> + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    use futures::FutureExt;
    tokio::spawn(async move {
        if std::panic::AssertUnwindSafe(task)
            .catch_unwind()
            .await
            .is_err()
        {
            tracing::error!(task = name, "background task panicked");
        }
    })
}

/// A background job run every `period` until its handle is aborted (or for the life of the
/// process): a panic in one run is logged and the next run happens anyway (a bare loop would
/// die silently). Aborting the handle also stops a run in flight.
pub(crate) fn every<F, Fut>(
    name: &'static str,
    period: std::time::Duration,
    job: F,
) -> tokio::task::JoinHandle<()>
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
            if std::panic::AssertUnwindSafe(job())
                .catch_unwind()
                .await
                .is_err()
            {
                tracing::error!(
                    job = name,
                    "background job panicked; it runs again next time"
                );
            }
        }
    })
}

/// Failed machine authentications per address and minute, before it waits.
const AUTH_FAILURES_PER_MINUTE: u32 = 10;

/// The device and settings routers, under `/api`: anyone signed in drives them, and so does a
/// service token (`auth::DeviceClient`: an app's from the LAN too when allowed, a wall
/// screen's through the tunnel). Admin-only routes inside add `AdminUser`, which a service
/// token never is; settings add `SettingsClient`, which a wall screen never is.
const DEVICE_ROUTES: [(&str, DeviceRouter); 11] = [
    ("/broadlink", routes::broadlink::router),
    ("/devices", routes::devices::router),
    ("/hue-lamps", routes::hue::router),
    ("/matter", routes::matter::router),
    ("/meross", routes::meross::router),
    ("/nabaztag", routes::nabaztag::router),
    ("/scenes", routes::scenes::router),
    ("/tempo", routes::tempo::router),
    ("/tv", routes::tv::router),
    ("/androidtv", routes::androidtv::router),
    ("/zigbee", routes::zigbee::router),
];

type DeviceRouter = fn() -> Router<AppState>;

/// Whether `path` is one of `DEVICE_ROUTES`.
fn is_device_path(path: &str) -> bool {
    path.strip_prefix("/api").is_some_and(|rest| {
        DEVICE_ROUTES.iter().any(|(prefix, _)| {
            rest.strip_prefix(prefix)
                .is_some_and(|r| r.is_empty() || r.starts_with('/'))
        })
    })
}

pub fn build_app(state: AppState) -> Router {
    // Every device and settings route needs someone signed in (or a service token), said
    // once here: a handler never has to remember it. Admin-only ones add `AdminUser`.
    // Outside: signing in and the sessions (`auth`, `passkeys`), the wall screens' pairing
    // (`/screens`, whose handlers name their own extractor), `/health`, the signed-out
    // `/api/sun` (day or night, for a page's theme) and the IR bridge (`ir`: its machine
    // route, and the keymap behind `AuthenticatedUser`).
    let signed_in = DEVICE_ROUTES
        .into_iter()
        .fold(Router::<AppState>::new(), |router, (prefix, routes)| {
            router.nest(prefix, routes())
        })
        .route_layer(middleware::from_extractor_with_state::<
            auth::DeviceClient,
            AppState,
        >(state.clone()));
    let api_router = Router::<AppState>::new()
        .merge(routes::root::api_router())
        .nest("/auth", routes::auth::router())
        .merge(routes::passkeys::router())
        .nest("/screens", routes::screens::router())
        .nest("/ir", routes::ir::router())
        .merge(signed_in);

    let app = Router::<AppState>::new()
        .merge(routes::root::health_router())
        .nest("/api", api_router);

    let csp = content_security_policy(&state.config.frontend_dist_dir);
    let app = if state.config.frontend_dist_dir.join("index.html").is_file() {
        app.fallback_service(
            ServeDir::new(state.config.frontend_dist_dir.clone()).not_found_service(
                ServeFile::new(state.config.frontend_dist_dir.join("index.html")),
            ),
        )
    } else {
        app.merge(routes::root::router())
    };

    // No CORS layer: no answer ever carries `Access-Control-Allow-Origin`, so a page elsewhere
    // cannot read one; a change it would send is refused by the guard. Inside out: the guard,
    // the extractors' rejections coded, the security headers (on the guard's refusals too),
    // the refusals logged.
    let gate = Gate {
        public_origin: state.config.public_url.clone(),
        allow_lan_tokens: state.config.allow_lan_tokens,
    };
    app.layer(middleware::from_fn(move |request, next| {
        guard_request(request, next, gate.clone())
    }))
    .layer(middleware::from_fn(coded_rejections))
    .layer(middleware::from_fn(move |request, next| {
        security_headers(request, next, csp.clone())
    }))
    .layer(middleware::from_fn(audit))
    // Last-resort guard so no request can hang a connection forever; the
    // limit sits above every legitimate long operation (IR learning,
    // device discovery, Zigbee command reply timeout of 120s).
    .layer(TimeoutLayer::with_status_code(
        axum::http::StatusCode::GATEWAY_TIMEOUT,
        std::time::Duration::from_secs(180),
    ))
    .layer(
        TraceLayer::new_for_http().make_span_with(|request: &axum::extract::Request| {
            tracing::info_span!(
                "http",
                method = %request.method(),
                uri = %loggable_path(request.uri().path()),
            )
        }),
    )
    .with_state(state)
}

/// What the request guard knows of the setup.
#[derive(Debug, Clone, Default)]
struct Gate {
    /// `PUBLIC_URL`, an origin (`https://home.example.com`): where the app's own pages are.
    public_origin: Option<String>,
    /// `ALLOW_LAN_TOKENS`: an app's token over the LAN's plain HTTP.
    allow_lan_tokens: bool,
}

/// What a request may do before any route sees it (`request_allowed`).
async fn guard_request(request: axum::extract::Request, next: Next, gate: Gate) -> Response {
    let peer = auth::peer_addr(request.extensions()).map(|addr| addr.ip());
    match request_allowed(
        peer,
        request.method(),
        request.uri().path(),
        request.headers(),
        &gate,
    ) {
        Ok(()) => next.run(request).await,
        // said in the log by `audit`, with the code
        Err(why) => AppError::from(why).into_response(),
    }
}

/// Whether `ip` is a Tailscale address (100.64.0.0/10, fd7a:115c:a1e0::/48): WireGuard
/// carries it encrypted, whatever the LAN.
fn is_tailscale(ip: std::net::IpAddr) -> bool {
    match ip.to_canonical() {
        std::net::IpAddr::V4(v4) => (v4.to_bits() & 0xffc0_0000) == 0x6440_0000,
        std::net::IpAddr::V6(v6) => (v6.to_bits() >> 80) == 0xfd7a_115c_a1e0,
    }
}

/// Three rules, before routing:
/// - From the LAN (not loopback, where cloudflared and the dev proxy are), only `/health`,
///   the IR bridge's `/api/ir/key`, and the device routes for an app's service token: over
///   Tailscale always, over the LAN's plain HTTP only with `ALLOW_LAN_TOKENS` (the token would
///   cross the Wi-Fi in clear). Everything else goes through the tunnel's HTTPS (`Use the
///   public address`). Passkeys cannot sign in over the LAN address anyway; this also keeps
///   people's sessions and cookies off cleartext HTTP.
/// - The IR bridge's `/api/ir/key` comes from the house only: through the tunnel (loopback,
///   with cloudflared's `CF-Connecting-IP`, or any proxy's forwarding header) it is refused,
///   whatever the token (`lan_only`).
/// - A request that changes something must come from this site: a browser says where it
///   comes from (`Sec-Fetch-Site`, else `Origin`, which must be `PUBLIC_URL`'s origin or
///   match `Host`; an `Origin` with neither is refused, fail closed); a sibling subdomain is
///   « same-site » but not this site. Clients that say nothing (kird, curl) pass.
fn request_allowed(
    peer: Option<std::net::IpAddr>,
    method: &axum::http::Method,
    path: &str,
    headers: &axum::http::HeaderMap,
    gate: &Gate,
) -> Result<(), Refusal> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let local = peer.is_none_or(|ip| ip.to_canonical().is_loopback());
    let forwarded = ["cf-connecting-ip", "x-forwarded-for", "x-real-ip"]
        .iter()
        .any(|name| headers.contains_key(*name));
    if path == "/api/ir/key" && local && forwarded {
        return Err(Refusal::LanOnly);
    }
    let encrypted = peer.is_some_and(is_tailscale);
    let machine = || {
        auth::has_service_token(headers)
            && is_device_path(path)
            && (encrypted || gate.allow_lan_tokens)
    };
    if !local && !matches!(path, "/health" | "/api/ir/key") && !machine() {
        return Err(Refusal::UsePublicAddress);
    }
    if matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    ) {
        return Ok(());
    }
    match header("sec-fetch-site") {
        Some("same-origin" | "none") => Ok(()),
        Some(_) => Err(Refusal::CrossSite),
        None => match header("origin") {
            None => Ok(()),
            Some(origin) => {
                let ours = gate.public_origin.as_deref() == Some(origin);
                let same_host = header("host")
                    .is_some_and(|host| origin.split_once("://").map(|(_, h)| h) == Some(host));
                if ours || same_host {
                    Ok(())
                } else {
                    Err(Refusal::CrossSite)
                }
            }
        },
    }
}

/// Every request notes who asked (`auth::Principal`, filled by the extractors); every 401 and
/// 403 is logged with its code, the path, who and the client's address (never a secret: the
/// path is `loggable_path`'s, a token is named, never shown).
async fn audit(mut request: axum::extract::Request, next: Next) -> Response {
    let path = loggable_path(request.uri().path()).into_owned();
    let ip = auth::request_ip(request.extensions(), request.headers());
    let principal = auth::Principal::default();
    request.extensions_mut().insert(principal.clone());
    let response = next.run(request).await;
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403) {
        let code = response
            .extensions()
            .get::<error::Refused>()
            .map_or("uncoded", |r| r.0);
        tracing::warn!(
            status = status.as_u16(),
            code,
            %path,
            who = principal.who().as_deref().unwrap_or("nobody"),
            %ip,
            "refused"
        );
    }
    response
}

/// An extractor's own refusal (a body that is not the JSON a route reads, a path or a query
/// that does not parse, a body too large) is axum's plain text: under `/api` it becomes the
/// coded answer every refusal is (`invalid`, `too_large`), with axum's status, its detail in
/// the log only.
async fn coded_rejections(request: axum::extract::Request, next: Next) -> Response {
    let api = request.uri().path().starts_with("/api");
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let plain = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| t.starts_with("text/plain"));
    if !api || !plain || !matches!(status, 400 | 413 | 415 | 422) {
        return response;
    }
    let detail = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    tracing::debug!(status, %detail, "a request the route could not read");
    let code = if status == 413 {
        Refusal::TooLarge
    } else {
        Refusal::Invalid
    };
    // axum's status stays (422: the JSON's types; 415: no JSON at all): the code says the rest
    let status =
        axum::http::StatusCode::from_u16(status).unwrap_or(axum::http::StatusCode::BAD_REQUEST);
    AppError::coded(status, code.as_str(), code.message()).into_response()
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
/// searched for, is left out altogether). Today's links carry it in their fragment, which no
/// request holds; the old `/invite/<token>` pages and `/api/invites/<token>` reads are still
/// redacted, should one come.
fn loggable_path(path: &str) -> std::borrow::Cow<'_, str> {
    for prefix in ["/api/invites/", "/invite/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            if rest.len() > 16 {
                return format!("{prefix}…").into();
            }
        }
    }
    path.into()
}

async fn security_headers(
    request: axum::extract::Request,
    next: Next,
    csp: header::HeaderValue,
) -> Response {
    let cache = cache_policy(request.uri().path());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static(cache),
    );
    let fixed = header::HeaderValue::from_static;
    headers.insert("cross-origin-opener-policy", fixed("same-origin"));
    headers.insert(
        "permissions-policy",
        fixed("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    headers.insert(header::X_FRAME_OPTIONS, fixed("DENY"));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, fixed("nosniff"));
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        fixed("max-age=63072000; includeSubDomains"),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, csp);
    headers.insert(
        header::REFERRER_POLICY,
        fixed("strict-origin-when-cross-origin"),
    );
    response
}

/// The page's Content-Security-Policy. Scripts: our own files, plus exactly the inline
/// scripts of the built `index.html` (SvelteKit's boot script, the theme set before first
/// paint), allowed by their SHA-256 — never `'unsafe-inline'`. Styles keep `'unsafe-inline'`:
/// Svelte's `style:` directives write inline styles (a progress bar's width, a colour swatch).
fn content_security_policy(dist_dir: &std::path::Path) -> header::HeaderValue {
    let index = dist_dir.join("index.html");
    let page = match std::fs::read_to_string(&index) {
        Ok(page) => page,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(path = %index.display(), "no built web app: the CSP allows no inline script");
            String::new()
        }
        Err(error) => {
            // the page would be served and its boot script refused: said loudly
            tracing::error!(%error, path = %index.display(), "index.html unreadable: the CSP allows none of its inline scripts, the web app will not start");
            String::new()
        }
    };
    let hashes: String = inline_scripts(&page)
        .map(|script| {
            use base64::Engine;
            use sha2::Digest;
            let digest = sha2::Sha256::digest(script.as_bytes());
            format!(
                " 'sha256-{}'",
                base64::engine::general_purpose::STANDARD.encode(digest)
            )
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
        jwt_secret_problem(&self.config.jwt_secret)
            .or_else(|| ir_token_problem(self.config.ir_api_token.as_deref()))
            .map_or(Ok(()), |why| {
                Err(AppError::http(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Refusing to start: {why}"),
                ))
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
    const PLACEHOLDERS: &[&str] = &[
        config::DEFAULT_JWT_SECRET,
        "change-me",
        "changeme",
        "secret",
    ];
    if PLACEHOLDERS.iter().any(|p| secret.eq_ignore_ascii_case(p)) {
        Some("JWT_SECRET is a placeholder; set a random one in .env (openssl rand -hex 32)")
    } else if secret.len() < 32 {
        Some("JWT_SECRET is shorter than 32 bytes; set a random one in .env (openssl rand -hex 32)")
    } else {
        None
    }
}

/// Why `token` cannot open the IR route: it fires scenes, the AC, the TV and the shutters, so
/// a guessable one is a remote control for anyone on the Wi-Fi. At least 32 characters; unset
/// (or empty) turns the route off.
fn ir_token_problem(token: Option<&str>) -> Option<&'static str> {
    token.filter(|t| t.len() < 32).map(|_| {
        "IR_API_TOKEN is shorter than 32 characters; set a random one in .env and kird.conf (openssl rand -hex 32), or leave it empty to turn the IR route off"
    })
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

    fn allowed(
        peer: Option<std::net::IpAddr>,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<(), Refusal> {
        request_allowed(peer, method, path, headers, &Gate::default())
    }

    #[test]
    fn the_lan_reaches_only_health_and_the_ir_bridge() {
        let lan = Some("192.168.1.20".parse().unwrap());
        let tunnel = Some("127.0.0.1".parse().unwrap());
        let none = HeaderMap::new();
        assert!(allowed(lan, &Method::POST, "/api/ir/key", &none).is_ok());
        assert!(allowed(lan, &Method::GET, "/health", &none).is_ok());
        assert_eq!(
            allowed(lan, &Method::GET, "/api/meross", &none),
            Err(Refusal::UsePublicAddress)
        );
        assert!(allowed(lan, &Method::GET, "/", &none).is_err());
        assert!(allowed(tunnel, &Method::GET, "/api/meross", &none).is_ok());
        assert!(allowed(
            Some("::ffff:127.0.0.1".parse().unwrap()),
            &Method::GET,
            "/x",
            &none
        )
        .is_ok());
    }

    #[test]
    fn the_ir_bridge_is_refused_through_the_tunnel() {
        let tunnel = Some("127.0.0.1".parse().unwrap());
        let from_cloudflare = headers(&[("cf-connecting-ip", "203.0.113.9")]);
        assert_eq!(
            allowed(tunnel, &Method::POST, "/api/ir/key", &from_cloudflare),
            Err(Refusal::LanOnly)
        );
        let proxied = headers(&[("x-forwarded-for", "203.0.113.9")]);
        assert_eq!(
            allowed(tunnel, &Method::POST, "/api/ir/key", &proxied),
            Err(Refusal::LanOnly)
        );
        assert!(
            allowed(tunnel, &Method::POST, "/api/ir/key", &HeaderMap::new()).is_ok(),
            "kird on the Pi itself"
        );
    }

    #[test]
    fn an_apps_token_comes_over_tailscale_and_over_the_lan_only_when_allowed() {
        let service = headers(&[("authorization", "Bearer maison_abc")]);
        let wifi = Some("192.168.1.20".parse().unwrap());
        assert_eq!(
            allowed(wifi, &Method::GET, "/api/meross", &service),
            Err(Refusal::UsePublicAddress),
            "the token would cross the Wi-Fi in clear"
        );
        let lan_ok = Gate {
            allow_lan_tokens: true,
            ..Gate::default()
        };
        assert!(request_allowed(wifi, &Method::GET, "/api/meross", &service, &lan_ok).is_ok());
        for tailscale in ["100.64.0.7", "100.127.255.1", "fd7a:115c:a1e0::5"] {
            assert!(
                allowed(
                    Some(tailscale.parse().unwrap()),
                    &Method::GET,
                    "/api/meross",
                    &service
                )
                .is_ok(),
                "{tailscale}"
            );
        }
        assert!(!is_tailscale("100.128.0.1".parse().unwrap()));
        assert!(!is_tailscale("fd7a:115c:a1e1::1".parse().unwrap()));
    }

    #[test]
    fn a_service_token_reaches_the_device_routes_from_the_lan_and_nothing_else() {
        let lan = Some("100.64.0.7".parse().unwrap());
        let service = headers(&[("authorization", "Bearer maison_abc")]);
        let session = headers(&[("authorization", "Bearer eyJhbGciOiJIUzI1NiJ9.x.y")]);
        for path in [
            "/api/zigbee/lamps",
            "/api/zigbee/lamps/1/power",
            "/api/matter/covers",
            "/api/tv",
            "/api/scenes/x/run",
        ] {
            assert!(
                allowed(lan, &Method::POST, path, &service).is_ok(),
                "{path}"
            );
            assert!(
                allowed(lan, &Method::POST, path, &session).is_err(),
                "{path}: a session goes through the tunnel"
            );
        }
        for path in [
            "/api/people",
            "/api/invites",
            "/api/passkeys",
            "/api/auth/verify",
            "/api/ir/keymap",
            "/api/zigbeex",
            "/",
        ] {
            assert!(
                allowed(lan, &Method::GET, path, &service).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn a_change_must_come_from_this_site() {
        let local = Some("127.0.0.1".parse().unwrap());
        let post = |h: &[(&'static str, &str)]| {
            allowed(local, &Method::POST, "/api/meross/x/toggle", &headers(h))
        };
        assert!(post(&[("sec-fetch-site", "same-origin")]).is_ok());
        assert_eq!(
            post(&[("sec-fetch-site", "cross-site")]),
            Err(Refusal::CrossSite)
        );
        assert!(
            post(&[("sec-fetch-site", "same-site")]).is_err(),
            "a sibling subdomain is not this site"
        );
        assert!(post(&[
            ("origin", "https://evil.example"),
            ("host", "home.kahn.studio")
        ])
        .is_err());
        assert!(post(&[
            ("origin", "https://home.kahn.studio"),
            ("host", "home.kahn.studio")
        ])
        .is_ok());
        assert!(post(&[]).is_ok(), "kird and curl say nothing");
        assert!(
            post(&[("origin", "https://home.kahn.studio")]).is_err(),
            "an origin and no host to compare it with: refused"
        );
        assert!(post(&[("origin", "null")]).is_err());
        let public = Gate {
            public_origin: Some("https://home.kahn.studio".into()),
            ..Gate::default()
        };
        let with_public = |h: &[(&'static str, &str)]| {
            request_allowed(local, &Method::POST, "/api/x", &headers(h), &public)
        };
        assert!(
            with_public(&[("origin", "https://home.kahn.studio")]).is_ok(),
            "PUBLIC_URL's origin, no Host needed (HTTP/2)"
        );
        assert_eq!(
            with_public(&[
                ("origin", "https://evil.example"),
                ("host", "home.kahn.studio")
            ]),
            Err(Refusal::CrossSite)
        );
        assert!(with_public(&[("origin", "https://evil.example")]).is_err());
        assert!(
            allowed(
                local,
                &Method::GET,
                "/api/x",
                &headers(&[("sec-fetch-site", "cross-site")])
            )
            .is_ok(),
            "reads are harmless"
        );
    }

    #[test]
    fn a_short_ir_token_is_refused_and_none_turns_the_route_off() {
        assert!(ir_token_problem(Some("test-ir-token")).is_some());
        assert!(ir_token_problem(Some(&"k".repeat(32))).is_none());
        assert!(ir_token_problem(None).is_none());
    }

    #[test]
    fn every_path_says_how_long_it_may_be_kept() {
        assert_eq!(cache_policy("/api/tempo"), "no-store");
        assert_eq!(
            cache_policy("/_app/immutable/chunks/app.4f2a.js"),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(
            cache_policy("/brand.svg"),
            "no-cache",
            "a new logo shows at the next visit"
        );
        assert_eq!(cache_policy("/"), "no-cache");
    }

    #[test]
    fn invitation_tokens_stay_out_of_the_logs() {
        assert_eq!(
            loggable_path(&format!("/api/invites/{}", "a".repeat(64))),
            "/api/invites/…"
        );
        assert_eq!(
            loggable_path("/api/invites/0123456789abcdef"),
            "/api/invites/0123456789abcdef",
            "an invitation's id is not secret"
        );
        assert_eq!(
            loggable_path(&format!("/invite/{}", "a".repeat(43))),
            "/invite/…",
            "an old link's page"
        );
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
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    };

    #[tokio::test(start_paused = true)]
    async fn a_panicking_run_is_followed_by_the_next_and_abort_stops_it() {
        let runs = Arc::new(AtomicU32::new(0));
        let counted = runs.clone();
        let handle = super::every("test", std::time::Duration::from_secs(1), move || {
            let n = counted.fetch_add(1, Ordering::SeqCst);
            async move { assert!(n != 1, "the second run panics") }
        });
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        assert_eq!(
            runs.load(Ordering::SeqCst),
            4,
            "runs at 0, 1 (panics), 2 and 3 s"
        );
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
        assert!(
            csp.contains("script-src 'self' 'sha256-qVpDBgj7bpq5hMAcGp3AOc79J3Y1Z4HvySTwKrWDoy4='"),
            "{csp}"
        );
        let scripts = csp
            .split(';')
            .find(|d| d.trim_start().starts_with("script-src"))
            .unwrap();
        assert!(!scripts.contains("unsafe-inline"), "{scripts}");
    }

    #[test]
    fn without_a_built_frontend_only_our_own_scripts_run() {
        let csp = content_security_policy(std::path::Path::new("/nonexistent"));
        assert!(csp.to_str().unwrap().contains("script-src 'self';"));
    }
}
