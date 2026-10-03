# Passkeys

Sign-in is by passkey only (WebAuthn), Ariane's implementation reused as is: same libraries,
ceremonies, rules and screens. Ariane's own passkey notes hold the background
reading; this file says what Maison does and where it differs.

Code: `backend/src/passkey.rs` (ceremonies, limits, relying party), `people.rs` (people,
passkeys, invitations), `routes/passkeys.rs`, `auth.rs` (sessions); `web/src/lib/passkeys.ts`,
the sign-in screen, `routes/invite/[token]`, `routes/account`.

## Shared with Ariane

- `webauthn-rs` 0.5.5 (`default-features = false, features = ["conditional-ui"]`) and
  `webauthn-rs-proto` at the same version, to force `residentKey: "required"`: a discoverable
  credential, so signing in asks for no name. `userVerification: "required"`,
  `attestation: "none"`, `excludeCredentials` filled.
- Ceremony states in memory: single use, 5 minutes, 10 000 at most, 20 per address.
- Failures limited to 10 a minute per address (an IPv6 address counts as its /64); ceremonies
  started signed out to 30 a minute.
- Invitations: 256 random bits, only the hash kept, 7 days, single use. An invitation for a
  person who exists adds a passkey (lost phone): same person, same user handle, an admin
  stays one.
- The last passkey cannot be removed; removing one ends the other sessions.
- Web: `passkeys.ts` (the session, the people, the invitations and the passkeys' API; JSON
  level 3 with a base64url fallback, `signalUnknownCredential`), a one-button sign-in screen,
  the invitation page, « Mon compte » (`web/src/lib/account/`: PasskeyList, People, Invites).
- Tests: 1Password's software authenticator (`passkey` 0.6) end to end in Rust
  (`backend/tests/passkey.rs`); Chromium's virtual authenticator (CDP `WebAuthn`) in
  `e2e/passkeys.ts`.

## Different in Maison

- **No database**: `auth/auth.json` (people, passkeys, invitations) is read on every use and
  replaced whole through `store.rs` (temp file + rename, 0600, owner kept, cross-process lock)
  in `auth/`, the service's own 0700 directory, because `maison-backend invite` runs as root
  on the Pi while the service writes the same file.
- **Sessions**: Maison's own. A JWT access token (15 min) that says only who and when, and a
  rotating refresh token (7 days) stored hashed, spent atomically; a replayed one ends its
  family. Name and role are read from `people.rs` on every request; `not_before_ms` ends a
  person's sessions at once. Cookies are `__Host-`, HttpOnly, Secure, SameSite=Lax. A
  request that changes something must be same-site (`Sec-Fetch-Site`, else `Origin` vs
  `Host`).
- **Origin**: `PUBLIC_URL`, else `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`. The RP ID is its
  host and cannot change without losing every passkey. An IP address is never an RP ID: the
  LAN address cannot sign in, so the house goes through the tunnel too. Locally,
  `http://localhost` is a secure context and works.
- **Client address**: cloudflared runs on the Pi, so a loopback request names its client in
  `CF-Connecting-IP` (else `X-Real-IP`, else the last `X-Forwarded-For`); from anywhere else
  these headers are ignored.
- **OpenSSL**: `webauthn-rs-core` links it; for the static musl binary, `openssl` with
  `features = ["vendored"]`, compiled by `cargo zigbuild` for `arm-unknown-linux-musleabihf`.
- **Roles**: `admin` invites, lists and removes people, and configures (device addresses and
  pairing, APK installs, Matter commissioning, the house's place, the remote's keymap);
  `member` does everything else, driving the house included.
- **Edition 2021**: no let-chains; Ariane's are rewritten as nested `if`s.
