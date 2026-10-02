use std::net::SocketAddr;

use maison_backend::app_parts_from_env;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("invite") {
        std::process::exit(invite(&args[1..]).await);
    }
    init_tracing();

    let (app, state) = app_parts_from_env().expect("failed to build app");
    state
        .validate_runtime_security()
        .expect("insecure runtime configuration");
    let addr: SocketAddr = state
        .config
        .listen_address()
        .parse()
        .expect("invalid listen address");
    let listener = TcpListener::bind(addr).await.expect("bind failed");

    tracing::info!(address = %listener.local_addr().expect("local addr"), "listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");

    state.shutdown().await;
}

const INVITE_USAGE: &str = "usage: maison-backend invite <person> --name <Name> [--admin]
  A one-time link (7 days) that registers <person>'s passkey: their first one, or a new
  one after losing them all. <person> is an id: lowercase letters, digits and dashes.";

/// `maison-backend invite leonard --name Léonard --admin`: prints the link. The first
/// account, and the way back in; the same as an admin's « Inviter quelqu'un ».
async fn invite(args: &[String]) -> i32 {
    let mut person = None;
    let mut name = None;
    let mut admin = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--admin" => admin = true,
            "--name" => name = rest.next().cloned(),
            "-h" | "--help" => {
                println!("{INVITE_USAGE}");
                return 0;
            }
            other if person.is_none() && !other.starts_with('-') => person = Some(other.to_string()),
            _ => {
                eprintln!("{INVITE_USAGE}");
                return 2;
            }
        }
    }
    let (Some(person), Some(name)) = (person, name) else {
        eprintln!("{INVITE_USAGE}");
        return 2;
    };
    let config = maison_backend::config::Config::from_env();
    let Some(public_url) = config.public_url.as_deref() else {
        eprintln!("Set PUBLIC_URL (or CLOUDFLARE_PUBLIC_HOSTNAME): the link points there.");
        return 1;
    };
    if let Err(error) = maison_backend::passkey::relying_party(public_url) {
        eprintln!("{error}");
        return 1;
    }
    let people = maison_backend::people::People::new(&config.auth_path);
    match maison_backend::routes::passkeys::new_invite(&people, public_url, &person, &name, admin, None).await {
        Ok(created) => {
            println!("Invitation for {name}, valid {} days. The link, to open once:", maison_backend::passkey::INVITE_DAYS);
            println!("{}", created.url);
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        signal.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received");
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "maison_backend=info,tower_http=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}
