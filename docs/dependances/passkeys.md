# Clés d'accès (passkeys, WebAuthn)

Maison reprend telle quelle l'implémentation d'Ariane : mêmes bibliothèques, mêmes
cérémonies, mêmes règles, mêmes écrans. Les notes de lecture (l'état de l'art, ce que fait
`webauthn-rs` 0.5.5 et pourquoi chaque option) sont dans Ariane,
`docs/dependances/passkeys.md` ; ici, seulement ce qui diffère.

## Ce qui est pareil

- `webauthn-rs` 0.5.5, `default-features = false, features = ["conditional-ui"]`, et
  `webauthn-rs-proto` à la même version pour forcer `residentKey: "required"` (clé
  découvrable : la connexion ne demande aucun nom).
- `userVerification: "required"`, `attestation: "none"`, `excludeCredentials` rempli.
- États des cérémonies en mémoire, à usage unique, 5 minutes, 10 000 au plus, 20 par adresse.
- Échecs limités à 10 par minute et par adresse (une adresse IPv6 compte pour son /64),
  cérémonies commencées hors session à 30 par minute.
- Invitations : 256 bits aléatoires, seul le hash est gardé, 7 jours, usage unique. Une
  invitation pour une personne qui existe ajoute une clé (perte de téléphone) : même
  personne, même user handle, un admin le reste.
- La dernière clé ne se retire pas ; retirer une clé ferme les autres sessions.
- Côté web : `passkeys.ts` (JSON niveau 3 avec repli base64url, `signalUnknownCredential`),
  l'écran de connexion à un bouton, la page d'invitation, « Mon compte ».
- Tests : l'authentificateur logiciel de 1Password (`passkey` 0.6, `rust-crypto`) de bout en
  bout côté Rust (`backend/tests/passkey.rs`), l'authentificateur virtuel de Chromium (CDP
  `WebAuthn`) en e2e (`e2e/passkeys.ts`).

## Ce qui diffère

- **Pas de base SQLite** : `auth/auth.json` (personnes, clés, invitations), relu à chaque
  usage et remplacé d'un bloc (fichier temporaire + rename, 0600) dans `auth/`, un dossier
  du service (comme `matter/` : le dossier de l'app est à root), en gardant le propriétaire,
  parce que `maison-backend invite` tourne en root sur le Pi pendant que le service écrit
  le même fichier.
- **Sessions** : celles de Maison (jeton d'accès JWT 15 min + jeton de rafraîchissement
  tournant 7 jours, cookies `HttpOnly`). Le rafraîchissement relit la personne.
- **Adresse** : `PUBLIC_URL`, sinon `https://` + `CLOUDFLARE_PUBLIC_HOSTNAME`
  (`home.kahn.studio`). L'RP ID est son hôte : il ne change pas sans perdre toutes les clés.
  Une adresse IP n'est jamais un RP ID : `http://192.168.1.103:3033` ne peut pas se
  connecter, il faut passer par le tunnel, à la maison aussi.
- **Adresse du client** : cloudflared tourne sur le Pi, donc une requête de loopback dit
  pour qui elle vient dans `CF-Connecting-IP` (sinon `X-Real-IP`, sinon le dernier
  `X-Forwarded-For`) ; d'ailleurs, ces en-têtes ne sont pas crus.
- **OpenSSL** : `webauthn-rs-core` le lie ; pour le binaire musl statique du Pi 1,
  `openssl = { features = ["vendored"] }`, compilé par `cargo zigbuild` (vérifié le
  2026-10-02 pour `arm-unknown-linux-musleabihf`).
- **Rôles** : `admin` invite, `member` fait tout le reste (piloter la maison compris).
- **Édition 2021** : pas de let-chains, réécrites en `if` imbriqués.
