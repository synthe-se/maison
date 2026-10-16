# Passkeys

Sign-in is by passkey only (WebAuthn), Ariane's implementation reused as is: same libraries,
ceremonies, rules and screens. Ariane's own passkey notes hold the background
reading; this file says what Maison does and where it differs.

Code: `backend/src/passkey.rs` (ceremonies, limits, relying party), `people.rs` (people,
passkeys, invitations), `routes/passkeys.rs`, `auth.rs` (who is asking), `routes/auth.rs`
(the session's routes); `web/src/lib/passkeys.ts`, the sign-in screen, the invitation page,
`routes/account`.

## Shared with Ariane

- `webauthn-rs` 0.5.5 (`default-features = false, features = ["conditional-ui"]`) and
  `webauthn-rs-proto` at the same version, to force `residentKey: "required"`: a discoverable
  credential, so signing in asks for no name. `userVerification: "required"`,
  `attestation: "none"`, `excludeCredentials` filled.
- Ceremony states in memory: single use, 5 minutes, 10 000 at most, 20 per address.
- Failures limited to 10 a minute per address (an IPv6 address counts as its /64); ceremonies
  started signed out to 30 a minute.
- Invitations: 256 random bits, only the hash kept, single use, 7 days (72 hours for an
  admin's: a link that makes an admin should not lie around). An invitation for a person who
  exists adds a passkey (lost phone): same person, same user handle, an admin stays one; it
  ends that person's other sessions once used.
- The link is `<PUBLIC_URL>/invite#<token>`: the token rides in the fragment, which no request
  carries (logs, proxies, the tunnel never see it); the page asks who it is for with `POST
  /api/invites/greet` `{token}`.
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
- **Sessions**: Maison's own. A JWT access token (15 min, HS256 through our own provider:
  jsonwebtoken without its crypto backend) that says only who, when, and from which sign-in
  (`sid`, its refresh family), and a rotating refresh token (7 days) stored hashed, spent
  atomically; a replayed one ends its family. A session lives `SESSION_MAX_DAYS` (30) from
  its passkey sign-in, refreshed or not: then a passkey again. Name and role are read from
  `people.rs` on every request; `not_before_ms` ends a person's sessions at once, and an
  ended sign-in ends its access tokens too (a copied token dies with it). Cookies are
  `__Host-`, HttpOnly, Secure, SameSite=Lax.
- **Session routes**: `GET /api/auth/sessions` lists my sign-ins (when, from where, from
  what; `current` marks this one), `DELETE /api/auth/sessions/{id}` ends one; `POST
  /api/auth/logout` ends this one, `logout-everywhere` all of mine. Signing out empties the
  cookies and answers `Clear-Site-Data: "storage"` (what the page stored; not `"cookies"`,
  which would clear sibling sites' on the registrable domain). An admin ends someone's
  sessions with `POST /api/people/{id}/sessions/end`.
- **A recent passkey sign-in** (`REAUTH_MS`, 5 minutes) for what could hand the house to
  someone else: adding, renaming or removing a passkey, an admin's or a recovery invitation,
  pairing a screen, removing a person (never oneself: `not_yourself`). A stolen session
  cookie alone is not enough; past it, `reauth_needed`.
- **Requests**: one that changes something must come from this site (`Sec-Fetch-Site`, else
  `Origin` equal to `PUBLIC_URL`'s origin or matching `Host`; an `Origin` with neither is
  refused, `cross_site`). Every 401 and 403 is logged with its code, the path, who and the
  client's address.
- **Origin**: `PUBLIC_URL`, else `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`. The RP ID is its
  host and cannot change without losing every passkey. An IP address is never an RP ID: the
  LAN address cannot sign in, so the house goes through the tunnel too. Locally,
  `http://localhost` is a secure context and works.
- **Client address**: cloudflared runs on the Pi, so a loopback request names its client in
  `CF-Connecting-IP` (else `X-Real-IP`, else the last `X-Forwarded-For`); from anywhere else
  these headers are ignored.
- **OpenSSL**: `webauthn-rs-core` links it; for the static musl binary, `openssl` with
  `features = ["vendored"]`, compiled by `cargo zigbuild` for `arm-unknown-linux-musleabihf`.
- **Roles**: `admin` invites, lists and removes people, ends their sessions, pairs and
  removes wall screens, and configures (device addresses and pairing, a forced Broadlink
  scan, APK installs, Matter commissioning, the house's place, the remote's keymap, a litter
  box's factory reset); `member` does everything else, driving the house included.
- **Edition 2021**: no let-chains; Ariane's are rewritten as nested `if`s.

## Service tokens (machines)

A companion app (Achi, on a phone) drives the devices without a passkey: a long-lived
service token, sent as `Authorization: Bearer maison_…`.

- Made, listed and revoked from the CLI only (as root on the Pi, like `invite`); the token is
  printed once, only its `hash_secret` is kept, in `auth/auth.json` (`tokens`). 256 random
  bits, base64url, prefixed `maison_`. A revoked token stops at the next request.

  ```bash
  ssh <pi> 'cd /opt/maison && ./backend/target/release/maison-backend token create achi'
  ssh <pi> 'cd /opt/maison && ./backend/target/release/maison-backend token list'
  ssh <pi> 'cd /opt/maison && ./backend/target/release/maison-backend token revoke achi'
  ```
- One scope, `devices`: every route under the device layer (`auth::DeviceClient`, the
  routers of `DEVICE_ROUTES` in `lib.rs`), as a member drives them. An app's token comes as a
  bearer header only; in a cookie it is not read. Anything that takes
  `AuthenticatedUser` or `AdminUser` (people, passkeys, invitations, sessions, the keymap,
  pairing, device settings, scene editing) answers 403 `forbidden` to a service token. An
  unknown or revoked one is 401 `not_signed_in`.
- The request guard lets an app's token reach the device routes over Tailscale
  (`http://<pi>:3033`, encrypted by WireGuard), and over the LAN's plain HTTP only with
  `ALLOW_LAN_TOKENS` (the token would cross the Wi-Fi in clear), not only through the tunnel;
  people's sessions and wall screens still go through the tunnel.
- The last use is dated (`lastUsedMs`, written at most once an hour: the SD card). Tokens
  never reach the logs (only the path is traced).

## Wall screens

A wall screen (the frame, `frame/`) is a browser paired once, by an admin, as a TV signs in to
an account (RFC 8628's device grant, in a page). No token is ever typed, copied, shown, or put
in an address. Code: `backend/src/screens.rs` (pending pairings), `routes/screens.rs`.

1. Signed out, the browser asks `POST /api/screens/pairing`: a code it shows (`ABCD-EFGH`, 8
   letters and digits nobody misreads), and a secret it keeps in the page's memory. Ten minutes,
   three at once per address, ten a minute.
2. An admin, signed in with a passkey, gives the code and a name (`POST /api/screens`, « Mon
   compte »): **from the screen's own network** (the same public address; an IPv6 one counts as
   its /64), or `pairing_elsewhere`. A code sent from across the internet (« type this code in
   Maison ») pairs nothing.
3. The screen, asking every 3 s with its secret (`POST /api/screens/pairing/claim`), gets its
   cookie once: `__Host-maison_screen`, HttpOnly, Secure, SameSite=Lax, 400 days, renewed at
   each start of its page by `POST /api/screens/me` (`GET` says who it is and changes
   nothing). A grant never claimed is revoked after the ten minutes.

A paired screen is a service token of kind `screen` (`auth.json` `tokens`, `kind`; tokens kept
before kinds are `app`s): the `devices` scope, never people nor settings, its use dated, listed
and revoked by an admin (`GET /api/screens`, `DELETE /api/screens/{id}`), or with `token revoke`.
`DeviceClient` takes, in order: an app's bearer token, someone signed in, a screen's cookie;
a screen's token is read from its cookie only, never as a header. What changes the house's
settings under the device layer (a lamp's or a shutter's name, the sun schedule, the rabbit's
reboot) takes `SettingsClient`, which a screen never is (`not_for_screens`): the frame on the
wall is anyone's in the room. A screen unseen for `SCREEN_IDLE_DAYS` (default 90; 0 keeps them)
is revoked: a frame thrown away or stolen stops opening the house. Like people's sessions, a
screen comes through the tunnel only.

## The IR bridge

The set-top box's `kird` sends its keys with `IR_API_TOKEN` as a bearer token
(`auth::MachineClient`): compared in constant time, 32 characters at least (shorter, the
backend refuses to start), failures limited per address. It is the house's only: `POST
/api/ir/key` is answered from the LAN and refused through the tunnel (`lan_only`), whatever
the token.
