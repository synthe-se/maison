//! Passkeys end to end, as in Ariane: invitation, registration, sign-in, several passkeys,
//! the last one kept, admins only to invite. The authenticator is 1Password's software one
//! (`passkey` crate), which makes discoverable credentials like iCloud Keychain or Google
//! Password Manager do.

mod common;

use std::collections::BTreeMap;

use axum::{
    Router,
    http::{Method, StatusCode},
};
use maison_backend::config::Config;
use passkey::authenticator::{Authenticator, UiHint, UserCheck, UserValidationMethod};
use passkey::client::{Client, DefaultClientData};
use passkey::crypto::rust_crypto::RustCryptoBackend;
use passkey::types::Passkey;
use passkey::types::ctap2::{Aaguid, Ctap2Error};
use passkey::types::webauthn::{CredentialCreationOptions, CredentialRequestOptions};
use serde_json::{Value, json};
use webauthn_rs::prelude::Url;

const ORIGIN: &str = "https://maison.example.com";

/// A person at the device: always there, always verified (Face ID, Touch ID, a PIN).
struct Verified;

#[async_trait::async_trait]
impl UserValidationMethod for Verified {
    type PasskeyItem = Passkey;

    async fn check_user<'a>(
        &self,
        _hint: UiHint<'a, Passkey>,
        presence: bool,
        verification: bool,
    ) -> Result<UserCheck, Ctap2Error> {
        Ok(UserCheck { presence, verification })
    }

    fn is_verification_enabled(&self) -> Option<bool> {
        Some(true)
    }

    fn is_presence_enabled(&self) -> bool {
        true
    }
}

/// A device with its passkey provider: it keeps one passkey for the site.
struct Device(Client<Option<Passkey>, Verified, RustCryptoBackend, public_suffix::PublicSuffixList, ()>, &'static str);

impl Device {
    fn new() -> Self {
        Self::at(ORIGIN)
    }

    /// A device used from another page (a look-alike subdomain, say).
    fn at(origin: &'static str) -> Self {
        Device(Client::new(Authenticator::new(Aaguid::new_empty(), None, Verified, RustCryptoBackend)), origin)
    }

    /// `navigator.credentials.create()`, then `toJSON()`.
    async fn create(&mut self, options: &Value) -> Value {
        let request: CredentialCreationOptions = serde_json::from_value(options.clone()).unwrap();
        let created = self.0.register(Url::parse(self.1).unwrap(), request, DefaultClientData).await.unwrap();
        serde_json::to_value(created).unwrap()
    }

    /// `navigator.credentials.get()`, then `toJSON()`.
    async fn get(&mut self, options: &Value) -> Value {
        let request: CredentialRequestOptions = serde_json::from_value(options.clone()).unwrap();
        let got = self.0.authenticate(Url::parse(self.1).unwrap(), request, DefaultClientData).await.unwrap();
        serde_json::to_value(got).unwrap()
    }
}

/// A browser: the app, and the cookies it was given.
struct Browser {
    app: Router,
    cookies: BTreeMap<String, String>,
}

impl Browser {
    fn new(app: &Router) -> Self {
        Browser { app: app.clone(), cookies: BTreeMap::new() }
    }

    async fn send(&mut self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let mut request = common::request(method, path, None, body);
        let jar = self.cookies.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ");
        if !jar.is_empty() {
            request.headers_mut().insert("cookie", jar.parse().unwrap());
        }
        request.headers_mut().insert("user-agent", "Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X)".parse().unwrap());
        let (status, headers, json) = common::respond(&self.app, request).await;
        for set in headers.get_all("set-cookie") {
            let (pair, _) = set.to_str().unwrap().split_once(';').unwrap();
            let (name, value) = pair.split_once('=').unwrap();
            if value.is_empty() {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.into(), value.into());
            }
        }
        (status, json)
    }

    async fn get(&mut self, path: &str) -> (StatusCode, Value) {
        self.send(Method::GET, path, None).await
    }

    async fn post(&mut self, path: &str, body: Value) -> (StatusCode, Value) {
        self.send(Method::POST, path, Some(body)).await
    }

    /// Registration, start to finish.
    async fn register(&mut self, device: &mut Device, invite: Option<&str>) -> (StatusCode, Value) {
        let (s, start) = self.post("/api/passkeys/register/start", json!({ "invite": invite })).await;
        assert_eq!(s, StatusCode::OK, "{start}");
        let credential = device.create(&start["options"]).await;
        self.post("/api/passkeys/register/finish", json!({ "ceremony": start["ceremony"], "credential": credential })).await
    }

    /// Sign-in with whichever passkey the device holds.
    async fn login(&mut self, device: &mut Device) -> (StatusCode, Value) {
        let (s, start) = self.post("/api/passkeys/login/start", json!({})).await;
        assert_eq!(s, StatusCode::OK, "{start}");
        let credential = device.get(&start["options"]).await;
        self.post("/api/passkeys/login/finish", json!({ "ceremony": start["ceremony"], "credential": credential })).await
    }

    async fn whoami(&mut self) -> Option<String> {
        let (s, v) = self.post("/api/auth/verify", json!({})).await;
        (s == StatusCode::OK).then(|| v["user"]["id"].as_str().unwrap().to_string())
    }
}

/// The test config with nobody in it yet: the people come from invitations here.
fn nobody(name: &str) -> Config {
    let config = common::isolated_config(name);
    std::fs::remove_file(&config.auth_path).expect("the seeded people go");
    config
}

/// The app on a fresh root, reached at `ORIGIN`, and the first invitation (as the CLI makes it).
async fn house() -> (Router, Config, String) {
    let config = Config { public_url: Some(ORIGIN.into()), ..nobody("maison-passkeys") };
    let app = common::app(config.clone());
    let link = invite(&config, "leonard", "Léonard", true).await;
    (app, config, link)
}

async fn invite(config: &Config, person: &str, name: &str, admin: bool) -> String {
    let people = maison_backend::people::People::new(&config.auth_path);
    let created = maison_backend::routes::passkeys::new_invite(&people, ORIGIN, person, name, admin, None).await.unwrap();
    created.url.rsplit('/').next().unwrap().to_string()
}

#[tokio::test]
async fn an_invitation_registers_a_passkey_that_signs_in_without_a_name() {
    let (app, _, token) = house().await;
    let mut phone = Device::new();
    let mut b = Browser::new(&app);

    let (s, greeting) = b.get(&format!("/api/invites/{token}")).await;
    assert_eq!((s, greeting["name"].as_str()), (StatusCode::OK, Some("Léonard")));

    let (s, registered) = b.register(&mut phone, Some(&token)).await;
    assert_eq!(s, StatusCode::OK, "{registered}");
    assert_eq!(registered["user"], json!({ "id": "leonard", "name": "Léonard", "role": "admin" }));
    assert_eq!(registered["passkey"]["name"], "iPhone", "named after the device");
    assert_eq!(b.whoami().await.as_deref(), Some("leonard"), "signed in by the registration");

    // the link is spent
    let (s, v) = b.get(&format!("/api/invites/{token}")).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::NOT_FOUND, Some("invite_invalid")));

    // out, then in again: no name, no password
    b.post("/api/auth/logout", json!({})).await;
    assert_eq!(b.whoami().await, None);
    let (s, v) = b.login(&mut phone).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["user"]["id"], "leonard");
    assert_eq!(b.whoami().await.as_deref(), Some("leonard"));

    // the refresh token rotates and still knows who
    let (s, v) = b.post("/api/auth/refresh", json!({})).await;
    assert_eq!((s, v["user"]["name"].as_str()), (StatusCode::OK, Some("Léonard")));
}

#[tokio::test]
async fn a_ceremony_answers_once_and_a_lookalike_page_gets_nothing() {
    let (app, _, token) = house().await;
    let mut phone = Device::new();
    let mut b = Browser::new(&app);
    b.register(&mut phone, Some(&token)).await;

    let (_, start) = b.post("/api/passkeys/login/start", json!({})).await;
    let credential = phone.get(&start["options"]).await;
    let finish = json!({ "ceremony": start["ceremony"], "credential": credential });
    assert_eq!(b.post("/api/passkeys/login/finish", finish.clone()).await.0, StatusCode::OK);
    let (s, v) = b.post("/api/passkeys/login/finish", finish).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::BAD_REQUEST, Some("ceremony_expired")), "single use");

    // a page on a look-alike origin: its passkey is not for this site
    let mut elsewhere = Device::at("https://maison.example.com.evil.example");
    let (_, start) = b.post("/api/passkeys/register/start", json!({})).await;
    let request: CredentialCreationOptions = serde_json::from_value(start["options"].clone()).unwrap();
    let refused = elsewhere.0.register(Url::parse(elsewhere.1).unwrap(), request, DefaultClientData).await;
    assert!(refused.is_err(), "the client refuses an RP ID that is not its origin's");
}

#[tokio::test]
async fn more_passkeys_the_last_one_stays_and_registered_ones_are_excluded() {
    let (app, _, token) = house().await;
    let mut phone = Device::new();
    let mut b = Browser::new(&app);
    b.register(&mut phone, Some(&token)).await;

    // a second device, signed in
    let mut laptop = Device::new();
    let (s, v) = b.register(&mut laptop, None).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let (_, keys) = b.get("/api/passkeys").await;
    let ids: Vec<String> = keys.as_array().unwrap().iter().map(|k| k["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 2);

    // the same device again: the options exclude both, which the browser enforces (e2e)
    let (_, start) = b.post("/api/passkeys/register/start", json!({})).await;
    assert_eq!(start["options"]["publicKey"]["excludeCredentials"].as_array().map(Vec::len), Some(2));
    assert_eq!(start["options"]["publicKey"]["authenticatorSelection"]["residentKey"], "required");

    // renamed, then removed; the last one cannot go
    let (s, v) = b.send(Method::PATCH, &format!("/api/passkeys/{}", ids[1]), Some(json!({ "name": "Mac" }))).await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::OK, Some("Mac")));
    assert_eq!(b.send(Method::DELETE, &format!("/api/passkeys/{}", ids[0]), None).await.0, StatusCode::NO_CONTENT);
    let (s, v) = b.send(Method::DELETE, &format!("/api/passkeys/{}", ids[1]), None).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::CONFLICT, Some("last_passkey")));
    assert_eq!(b.whoami().await.as_deref(), Some("leonard"), "this browser stays signed in");

    // the removed passkey no longer signs in: the app tells the device to forget it
    let (s, v) = Browser::new(&app).login(&mut phone).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::NOT_FOUND, Some("unknown_passkey")));
    assert_eq!(Browser::new(&app).login(&mut laptop).await.0, StatusCode::OK);

    // not signed in: no passkey to add
    let (s, _) = Browser::new(&app).post("/api/passkeys/register/start", json!({})).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admins_invite_and_a_member_cannot() {
    let (app, config, token) = house().await;
    let mut b = Browser::new(&app);
    b.register(&mut Device::new(), Some(&token)).await;

    let (s, created) = b.post("/api/invites", json!({ "name": "Alex" })).await;
    assert_eq!(s, StatusCode::OK, "{created}");
    let link = created["url"].as_str().unwrap();
    assert!(link.starts_with(&format!("{ORIGIN}/invite/")));
    let (_, pending) = b.get("/api/invites").await;
    assert_eq!(pending[0]["person"], "alex");

    let mut alex = Browser::new(&app);
    let (s, v) = alex.register(&mut Device::new(), link.rsplit('/').next()).await;
    assert_eq!((s, v["user"]["role"].as_str()), (StatusCode::OK, Some("member")));
    assert_eq!(alex.get("/api/invites").await.0, StatusCode::FORBIDDEN);
    assert_eq!(alex.post("/api/invites", json!({ "name": "Eve" })).await.0, StatusCode::FORBIDDEN);

    // lost every passkey: a new invitation for the same person gives the way back
    let again = invite(&config, "alex", "Alex", false).await;
    let mut new_phone = Device::new();
    let (s, v) = Browser::new(&app).register(&mut new_phone, Some(&again)).await;
    assert_eq!((s, v["user"]["id"].as_str()), (StatusCode::OK, Some("alex")));
    let (_, keys) = alex.get("/api/passkeys").await;
    assert_eq!(keys.as_array().unwrap().len(), 2, "the same person, one more passkey");
}

#[tokio::test]
async fn passkeys_are_off_without_a_public_address() {
    let app = common::app(Config { public_url: None, ..nobody("maison-passkeys-off") });
    let (s, v) = Browser::new(&app).post("/api/passkeys/login/start", json!({})).await;
    assert_eq!((s, v["code"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("passkey_off")));
}

#[tokio::test]
async fn secure_cookies_are_host_bound_and_a_replayed_refresh_token_ends_its_sign_in() {
    let config = Config { public_url: Some(ORIGIN.into()), auth_cookie_secure: true, ..nobody("maison-passkeys-secure") };
    let app = common::app(config.clone());
    let token = invite(&config, "leonard", "Léonard", true).await;
    let mut b = Browser::new(&app);
    b.register(&mut Device::new(), Some(&token)).await;
    let names: Vec<&str> = b.cookies.keys().map(String::as_str).collect();
    assert_eq!(names, ["__Host-maison_refresh", "__Host-maison_session"], "bound to this host, path /");

    let first = b.cookies["__Host-maison_refresh"].clone();
    assert_eq!(b.post("/api/auth/refresh", json!({})).await.0, StatusCode::OK);
    let second = b.cookies["__Host-maison_refresh"].clone();
    assert_ne!(first, second, "rotated");

    // someone replays the first one: refused, and the sign-in it came from ends
    let mut thief = Browser::new(&app);
    thief.cookies.insert("__Host-maison_refresh".into(), first);
    assert_eq!(thief.post("/api/auth/refresh", json!({})).await.0, StatusCode::UNAUTHORIZED);
    b.cookies.remove("__Host-maison_session");
    assert_eq!(b.post("/api/auth/refresh", json!({})).await.0, StatusCode::UNAUTHORIZED, "the rightful one too: sign in again");
}
