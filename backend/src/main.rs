use std::net::SocketAddr;

use maison_backend::app_parts_from_env;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // The settings: MAISON_SOURCE_ROOT's .env when a root is named (a host, a test's throwaway
    // root: never another house's file found above the working directory), else the nearest
    // one. None is fine (the environment says it all); one that cannot be read or parsed is not
    let loaded = match std::env::var_os("MAISON_SOURCE_ROOT") {
        Some(root) => dotenvy::from_path(std::path::Path::new(&root).join(".env")),
        None => dotenvy::dotenv().map(drop),
    };
    if let Err(error) = loaded
        && !error.not_found()
    {
        eprintln!(".env could not be read: {error}");
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("invite") => std::process::exit(invite(&args[1..]).await),
        Some("token") => std::process::exit(token(&args[1..]).await),
        Some("edge") => std::process::exit(edge(&args[1..])),
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

    // One signal stops both listeners
    let (stop, stopped) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = stop.send(true);
    });
    let until_stopped = |mut stopped: tokio::sync::watch::Receiver<bool>| async move {
        let _ = stopped.wait_for(|stop| *stop).await;
    };

    // The edge (edge.rs): the same app, behind its TLS both ways and its layer
    let edge = match state.config.edge_listen.as_deref() {
        None => None,
        Some(address) => {
            let address: SocketAddr = address.parse().expect("EDGE_LISTEN: not an address");
            let tls = maison_backend::edge::server_config(&state.config.edge_dir)
                .expect("the edge's certificates (maison-backend edge init)");
            let listener = maison_backend::edge::EdgeListener::bind(address, tls)
                .await
                .expect("edge bind failed");
            tracing::info!(address = %address, "edge listening");
            let app = app
                .clone()
                .layer(axum::middleware::from_fn(maison_backend::edge::from_edge));
            let stopped = stopped.clone();
            Some(tokio::spawn(async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<maison_backend::edge::EdgePeer>(),
                )
                .with_graceful_shutdown(until_stopped(stopped))
                .await
            }))
        }
    };

    let for_serving = stopped.clone();
    let serving = async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(until_stopped(for_serving))
        .await
        .expect("server error");
        if let Some(edge) = edge {
            edge.await.expect("edge task").expect("edge server error");
        }
    };
    // An event stream never ends by itself (a wall screen keeps one open): the connections
    // still open SHUTDOWN_GRACE after the signal are dropped, and the state is saved all the same
    let grace = async {
        until_stopped(stopped).await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    tokio::select! {
        () = serving => {}
        () = grace => tracing::warn!("connections still open after the grace: closed"),
    }

    state.shutdown().await;
}

/// How long open connections get to finish once the service is told to stop.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

const EDGE_USAGE: &str = "usage: maison-backend edge init <house dir> <proxy dir>
       maison-backend edge client <house dir> <client dir> <name>
  The edge's certificates (a reverse proxy elsewhere reaching EDGE_LISTEN): this host's in
  <house dir> (EDGE_DIR: ca.pem, cert.pem, key.pem), the proxy's in <proxy dir> (client.pem,
  client-key.pem, and ca.pem to trust this host by). A CA used once, its key never written:
  run it again to replace both sides.
  `client` adds another client (a wall frame on the LAN): its files in <client dir>, its own
  one-use CA added to <house dir>'s ca.pem (remove it there to refuse that client alone). The
  service reads ca.pem at its start.";

/// `maison-backend edge init edge/ proxy/`.
fn edge(args: &[String]) -> i32 {
    match args {
        [init, house, proxy] if init == "init" => {
            match maison_backend::edge::init(house.as_ref(), proxy.as_ref()) {
                Ok(()) => {
                    println!("The edge's certificates: {house} (this host), {proxy} (the proxy).");
                    audit("edge certificates made");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
        [client, house, dir, name] if client == "client" => {
            match maison_backend::edge::client(house.as_ref(), dir.as_ref(), name) {
                Ok(()) => {
                    println!(
                        "The edge's client « {name} »: {dir}; {house}/ca.pem trusts it (restart the service)."
                    );
                    audit(&format!("edge client « {name} » made"));
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
        _ => {
            eprintln!("{EDGE_USAGE}");
            2
        }
    }
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
        "{:.6} WARN maison_backend::cli: {event} (by the CLI, as {})\n",
        jiff::Timestamp::now(),
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

/// Who runs the CLI (root, on the host), for the log line.
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
        eprintln!("Set PUBLIC_URL: the link points there.");
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
                    jiff::Timestamp::from_millisecond(ms).map_or_else(
                        |_| "?".into(),
                        |d| maison_backend::util::house_date(d).to_string(),
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
