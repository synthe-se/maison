pub mod adb;
pub mod androidtv;
pub mod atvremote;
pub mod auth;
pub mod broadlink;
pub mod config;
pub mod error;
#[cfg(feature = "bluetooth")]
pub mod hue;
#[cfg(not(feature = "bluetooth"))]
#[path = "hue_stub.rs"]
pub mod hue;
pub mod ir;
pub mod matter;
pub mod meross;
pub mod mitsubishi_ir;
pub mod nabaztag;
pub mod passkey;
pub mod people;
pub mod philips_ir;
pub mod routes;
pub mod sun;
pub mod tempo;
pub mod tuya;
pub mod tv;
pub mod zigbee;
pub mod zigbee_native;

pub use tempo::TempoService;

use std::sync::Arc;

use axum::Router;
use axum::middleware::{self, Next};
use axum::http::header;
use axum::response::Response;
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
    pub(crate) matter: MatterManager,
    pub(crate) meross: MerossManager,
    pub(crate) nabaztag: NabaztagManager,
    pub(crate) tempo: TempoService,
    pub(crate) tuya: TuyaManager,
    pub(crate) tv: TvManager,
    pub(crate) androidtv: AndroidTvManager,
    pub(crate) zigbee: ZigbeeManager,
}

pub fn app_from_env() -> Result<Router, AppError> {
    let config = Arc::new(Config::from_env());
    build_app_from_config(config)
}

pub fn app_parts_from_env() -> Result<(Router, AppState), AppError> {
    let config = Arc::new(Config::from_env());
    build_app_parts_from_config(config)
}

pub fn build_app_from_config(config: Arc<Config>) -> Result<Router, AppError> {
    let (app, _) = build_app_parts_from_config(config)?;
    Ok(app)
}

pub fn build_app_parts_from_config(config: Arc<Config>) -> Result<(Router, AppState), AppError> {
    let people = Arc::new(People::new(&config.auth_path));
    let passkeys = PasskeyState::new(&config).map(Arc::new);
    let refresh_store = RefreshTokenStore::load(&config.refresh_tokens_path);
    let broadlink =
        BroadlinkManager::new(&config.broadlink_codes_path, &config.climate_state_path)?;
    let hue = HueManager::new(config.as_ref())?;
    let ir = IrManager::new(&config.ir_keymap_path)?;
    let matter = MatterManager::new(config.as_ref())?;
    let meross = MerossManager::new(&config.meross_devices_path)?;
    let nabaztag = NabaztagManager::new(
        &config.nabaztag_config_path,
        config.nabaztag_host.as_deref(),
    )?;
    let tempo = TempoService::new(config.source_root.clone())?;
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
    tokio::spawn(async move {
        let device_ids = startup_tuya
            .list_devices()
            .await
            .into_iter()
            .map(|device| device.id)
            .collect::<Vec<_>>();
        for device_id in device_ids {
            let _ = startup_tuya.connect_device(&device_id).await;
        }
    });

    // The shutters' sun schedule: a look every 30 s sends what fell due since the last one.
    let schedule = state.matter.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            schedule.run_schedule().await;
        }
    });

    // Mirror the daily Tempo colors on the Nabaztag. The push is two UDP
    // datagrams and idempotent, so it simply repeats every 15 minutes: that
    // also re-applies the colors after a rabbit reboot.
    let tempo_rabbit = state.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(900));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let config = tempo_rabbit.nabaztag.config().await;
            if !config.tempo_enabled || config.host.is_none() {
                continue;
            }
            match tempo_rabbit.tempo.get_tempo_data(false).await {
                Ok((data, _)) => {
                    if let Some(today) = data.today.color.as_deref() {
                        // Ears show tomorrow: the official color when RTE has
                        // published it, the model's prediction otherwise.
                        let (tomorrow, predicted) =
                            tempo_rabbit.tempo.tomorrow_color_or_predicted().await;
                        if predicted {
                            tracing::debug!(color = ?tomorrow, "using predicted color for tomorrow");
                        }
                        if let Err(error) = tempo_rabbit
                            .nabaztag
                            .push_tempo(today, tomorrow.as_deref())
                            .await
                        {
                            tracing::debug!(%error, "tempo push to the rabbit failed");
                        }
                    }
                }
                Err(error) => tracing::debug!(%error, "tempo data unavailable for rabbit push"),
            }
        }
    });

    let app = build_app(state.clone());
    Ok((app, state))
}

pub fn build_app(state: AppState) -> Router {
    let api_router = Router::<AppState>::new()
        .merge(routes::root::api_router())
        .nest("/auth", routes::auth::router())
        .merge(routes::passkeys::router())
        .nest("/broadlink", routes::broadlink::router())
        .nest("/devices", routes::devices::router())
        .nest("/hue-lamps", routes::hue::router())
        .nest("/ir", routes::ir::router())
        .nest("/matter", routes::matter::router())
        .nest("/meross", routes::meross::router())
        .nest("/nabaztag", routes::nabaztag::router())
        .nest("/tempo", routes::tempo::router())
        .nest("/tv", routes::tv::router())
        .nest("/androidtv", routes::androidtv::router())
        .nest("/zigbee", routes::zigbee::router());

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
                        uri = %request.uri(),
                    )
                }),
        )
        .with_state(state)
}

async fn security_headers(
    request: axum::extract::Request,
    next: Next,
    csp: header::HeaderValue,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        "max-age=63072000; includeSubDomains".parse().unwrap(),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, csp);
    headers.insert(
        header::REFERRER_POLICY,
        "strict-origin-when-cross-origin".parse().unwrap(),
    );
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
        "default-src 'self'; script-src 'self'{hashes}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'none'"
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
        if self.config.jwt_secret == config::DEFAULT_JWT_SECRET {
            return Err(AppError::http(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Refusing to start with the default JWT secret. Set JWT_SECRET in .env.",
            ));
        }

        Ok(())
    }

    pub async fn shutdown(&self) {
        self.hue.shutdown().await;
        self.zigbee.shutdown().await;
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
        let dir = std::env::temp_dir().join(format!("maison-csp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // echo -n "a()" | openssl dgst -sha256 -binary | base64
        std::fs::write(dir.join("index.html"), "<script>a()</script>").unwrap();
        let csp = content_security_policy(&dir);
        let csp = csp.to_str().unwrap();
        assert!(csp.contains("script-src 'self' 'sha256-qVpDBgj7bpq5hMAcGp3AOc79J3Y1Z4HvySTwKrWDoy4='"), "{csp}");
        let scripts = csp.split(';').find(|d| d.trim_start().starts_with("script-src")).unwrap();
        assert!(!scripts.contains("unsafe-inline"), "{scripts}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn without_a_built_frontend_only_our_own_scripts_run() {
        let csp = content_security_policy(std::path::Path::new("/nonexistent"));
        assert!(csp.to_str().unwrap().contains("script-src 'self';"));
    }
}
