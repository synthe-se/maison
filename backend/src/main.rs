use std::net::SocketAddr;

use maison_backend::app_parts_from_env;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // no .env is fine (the environment says it all); one that cannot be read or parsed is not
    if let Err(error) = dotenvy::dotenv() {
        if !error.not_found() {
            eprintln!(".env could not be read: {error}");
        }
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("invite") => std::process::exit(invite(&args[1..]).await),
        Some("token") => std::process::exit(token(&args[1..]).await),
        _ => {}
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
  A one-time link (7 days, 72 hours for an admin) that registers <person>'s passkey: their
  first one, or a new one after losing them all. <person> is an id: lowercase letters,
  digits and dashes.";

/// Where the service writes its log (`deploy/openrc/maison`), `MAISON_LOG_FILE` to say another:
/// the CLI's security events (an invitation, a token made or revoked) are added to it, so the
/// service's log tells everything that opened the house. Absent (a dev machine): not written.
const SERVICE_LOG: &str = "/var/log/maison.log";

/// A security event of the CLI's, in the service's log (never a secret: names and ids).
fn audit(event: &str) {
    let path = maison_backend::config::env_text("MAISON_LOG_FILE")
        .unwrap_or_else(|| SERVICE_LOG.to_string());
    let line = format!(
        "{} WARN maison_backend::cli: {event} (by the CLI, as {})\n",
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        invoker()
    );
    if std::path::Path::new(&path).is_file() {
        use std::io::Write;
        let appended = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        if let Err(error) = appended {
            eprintln!("{path}: the event could not be logged: {error}");
        }
    }
}

/// Who runs the CLI (root, on the Pi), for the log line.
fn invoker() -> String {
    std::env::var("SUDO_USER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "?".into())
}

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
            other if person.is_none() && !other.starts_with('-') => {
                person = Some(other.to_string())
            }
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
    match maison_backend::routes::passkeys::new_invite(
        &people, public_url, &person, &name, admin, None,
    )
    .await
    {
        Ok(created) => {
            let hours = maison_backend::passkey::invite_lifetime_ms(admin) / 3_600_000;
            println!("Invitation for {name}, valid {hours} hours. The link, to open once:");
            println!("{}", created.url);
            audit(&format!(
                "invitation made for {person} (admin: {admin}), id {}",
                created.id
            ));
            0
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

const TOKEN_USAGE: &str = "usage: maison-backend token create <name> [--scope devices]
       maison-backend token list
       maison-backend token revoke <name or id>
  A service token lets a machine (a companion app) drive the devices with
  `Authorization: Bearer <token>`, from the LAN too; never people, passkeys,
  invitations, tokens nor settings. Shown once at creation; only its hash is kept.";

/// `maison-backend token create achi`: prints the token, once. `list`, `revoke`.
async fn token(args: &[String]) -> i32 {
    let config = maison_backend::config::Config::from_env();
    let people = maison_backend::people::People::new(&config.auth_path);
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = match words.as_slice() {
        ["create", name, rest @ ..] if !name.starts_with('-') => {
            let scope = match rest {
                [] => Ok(maison_backend::people::Scope::Devices),
                ["--scope", scope] => scope.parse(),
                _ => {
                    eprintln!("{TOKEN_USAGE}");
                    return 2;
                }
            };
            let scope = match scope {
                Ok(scope) => scope,
                Err(error) => {
                    eprintln!("{error}");
                    return 2;
                }
            };
            maison_backend::auth::issue_service_token(
                &people,
                name,
                scope,
                maison_backend::people::TokenKind::App,
            )
            .await
            .map(|token| match token {
                Some(token) => {
                    println!(
                        "Service token « {} » (scope {}). Shown this once, keep it now:",
                        name.trim(),
                        scope.as_str()
                    );
                    println!("{token}");
                    audit(&format!("service token « {} » created", name.trim()));
                    0
                }
                None => {
                    eprintln!(
                        "A token is already named « {} »: revoke it first.",
                        name.trim()
                    );
                    1
                }
            })
        }
        ["list"] => people.tokens().await.map(|tokens| {
            if tokens.is_empty() {
                println!("No service tokens.");
            }
            for t in tokens {
                // the house's days, not UTC's
                let day = |ms: i64| {
                    chrono::DateTime::from_timestamp_millis(ms).map_or_else(
                        || "?".into(),
                        |d| {
                            d.with_timezone(&maison_backend::util::HOUSE_TZ)
                                .format("%Y-%m-%d")
                                .to_string()
                        },
                    )
                };
                let used = t.last_used_ms.map_or_else(|| "never".into(), day);
                println!(
                    "{}  {}  {}  {}  created {}  last used {used}",
                    t.id,
                    t.name,
                    t.kind.as_str(),
                    t.scope.as_str(),
                    day(t.created_ms)
                );
            }
            0
        }),
        ["revoke", key] => people.revoke_token(key).await.map(|gone| {
            if gone {
                println!("Revoked: « {key} » no longer opens anything.");
                audit(&format!("service token « {key} » revoked"));
                0
            } else {
                eprintln!("No service token named « {key} ».");
                1
            }
        }),
        ["-h" | "--help"] => {
            println!("{TOKEN_USAGE}");
            return 0;
        }
        _ => {
            eprintln!("{TOKEN_USAGE}");
            return 2;
        }
    };
    outcome.unwrap_or_else(|error| {
        eprintln!("{error}");
        1
    })
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
