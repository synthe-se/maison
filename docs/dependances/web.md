# La pile web de Maison (vérifiée le 2 octobre 2026)

Même pile et mêmes règles qu'Ariane (`../ariane/docs/dependances/web.md`, vérifiée le 29 septembre),
avec ce qui a changé depuis : SvelteKit 3 et Vitest 5 sont sortis entre-temps. Tout ce qui suit a été
vérifié en compilant ce dépôt (`bun run build`, `bun run check`, `bun run test`), pas seulement lu.

Versions épinglées (exactes, `bun add -d --exact`) :

| Paquet | Version | Note |
|---|---|---|
| svelte | 5.57.1 | |
| @sveltejs/kit | **3.0.0** | sorti le 1er octobre 2026 |
| @sveltejs/adapter-static | **4.0.0** | |
| @sveltejs/vite-plugin-svelte | 7.3.1 | |
| vite | 8.3.2 | |
| bits-ui | 2.19.4 | |
| @inlang/paraglide-js | 2.25.4 | |
| svelte-check | 4.7.6 | |
| typescript | 6.0.3 | Kit 3 exige TS 6 ; TS 7 seul casse svelte-check (Ariane § 9) |
| @fontsource-variable/fraunces | 5.3.0 | |
| vitest, @vitest/browser-playwright, @vitest/coverage-v8 | **5.0.3** | |
| vitest-browser-svelte | 3.1.0 | |
| playwright | 1.63.0 | aussi dans `e2e/` |
| axe-core | 4.13.0 | `e2e/` |

Point de départ : `bunx sv@1.0.1 create web --template minimal --types ts --add sveltekit-adapter="adapter:static" paraglide=…`,
puis le nettoyage SPA d'Ariane (pas de `hooks.server.ts`, pas de `reroute`, `ssr = false`).

## 1. SvelteKit 3 : ce qui change par rapport à Ariane

Source : [Migrating to SvelteKit 3](https://svelte.dev/docs/kit/migrating-to-sveltekit-3), et le code installé
(`node_modules/@sveltejs/kit`), qui fait foi.

- **`$lib` devient `#lib`**, par les `imports` de `package.json` (des sous-chemins Node), que `sv` écrit :
  ```json
  "imports": { "#lib": "./src/lib/index.ts", "#lib/*": "./src/lib/*" }
  ```
  Le chemin est pris tel quel : **chaque import nomme son fichier avec son extension**
  (`#lib/api.ts`, `#lib/live.svelte.ts`, `#lib/paraglide/messages.js`). Le `tsconfig` généré
  (`$app/tsconfig`) active `allowImportingTsExtensions`. `bun scripts/imports.fix.ts` ajoute les
  extensions manquantes.
- **`tsconfig.json` étend `$app/tsconfig`** (dans `node_modules/$app/`, écrit par `svelte-kit sync`).
- **`$app/environment` devient `$app/env`** ; `$app/stores` est retiré (`$app/state` seulement) ;
  `version.pollInterval` vaut une heure par défaut (Maison garde une minute, comme Ariane).
- **Variables publiques : `src/env.ts` et `defineEnvVars`.** L'astuce d'Ariane
  (`process.env.PUBLIC_BASE_LOCALE` dans `vite.config.ts`, lu par `%sveltekit.env.…%` dans `app.html`)
  **ne marche plus** : le build écrivait `<html lang="">`. Le placeholder lit désormais les
  variables déclarées dans `src/env.ts` :
  ```ts
  export const variables = defineEnvVars({
    PUBLIC_BASE_LOCALE: { public: true, static: true, schema: () => /* settings.json */ },
    PUBLIC_THEME_KEY: { public: true, static: true, schema: () => 'maison-theme' }
  });
  ```
  `static: true` les inline au build ; le code les lit par `import { PUBLIC_THEME_KEY } from '$app/env/public'`.
  Une valeur dite une fois sert à la fois au script d'avant-peinture d'`app.html` et au code.
- `adapter-static` 4 : même usage (`adapter({ fallback: 'index.html' })`).
- Pas de service worker (`serviceWorker: { register: false }`) : Maison montre de l'état vivant,
  un écran hors ligne mentirait ; c'était déjà le choix de l'app React (commit `51603b0`).

## 2. Le thème : `light-dark()`, chaque couleur écrite une fois

Ariane déclare chaque couleur sombre deux fois (sous `@media (prefers-color-scheme: dark)` et
sous `[data-theme="dark"]`). Avec [`light-dark()`](https://developer.mozilla.org/en-US/docs/Web/CSS/color_value/light-dark)
(Chrome 123, Safari 17.5, Firefox 120), la paire claire et sombre est sur une seule ligne et
`color-scheme` choisit le côté :

```css
:root { color-scheme: light dark; }
:root[data-theme="light"] { color-scheme: light; }
:root[data-theme="dark"] { color-scheme: dark; }
:root { --ground: light-dark(var(--raw-cloud), var(--raw-night)); }
```

Au passage, un défaut d'Ariane disparaît : thème clair choisi sur un système sombre, ses
`--status-*` restaient sombres (ils n'étaient pas redéfinis sous `[data-theme="light"]`).
`light-dark()` ne prend que des couleurs : les ombres gardent leur géométrie et leurs couleurs
passent par des jetons (`--shadow-near`, `--shadow-far`).

## 3. Les données vivantes sans TanStack Query

`#lib/live.svelte.ts` (≈ 100 lignes) remplace TanStack Query : une valeur par clé, partagée par
toutes les vues qui la lisent, rafraîchie à l'intervalle voulu (ou à un intervalle fonction de la
valeur : un volet qui roule est relu chaque seconde), arrêtée quand plus aucune vue n'est montée,
suspendue quand l'onglet est caché et relue quand il revient. Une dépendance de moins, et le
comportement que `docs/ux/tableau-de-bord.md` § 4 demande.

## 4. Vitest 5 en mode navigateur

Source : [Vitest 5 migration](https://vitest.dev/guide/migration), le modèle `sv add vitest`.

- Deux projets dans `vite.config.ts` (qui hérite désormais de la config racine, `extends: true` par défaut) :
  - `browser` : `src/**/*.svelte.test.ts` dans Chromium (Playwright). Composants (`vitest-browser-svelte`) et
    modules à runes (`*.svelte.ts`) s'y testent tels qu'ils tournent.
  - `node` : les autres `*.test.ts`.
- Hors du serveur de dev de Kit, le module généré `$app/env/public` lit `globalThis.__sveltekit_dev` :
  `src/test-setup.ts` le pose (vide : les valeurs `static` sont déjà inlinées).
- Changements de Vitest 5 qui comptent ici : `getByText` et `toHaveTextContent` sont exacts par
  défaut (`toMatchTextContent` pour un extrait) ; `vi.mock` seulement au niveau du module ;
  `expect.poll` rejette à l'échéance ; `clearMocks` vrai par défaut.
- `bun run test` (une passe), `bun run coverage` (v8, résumé texte et HTML).

## 5. Les e2e : comme Ariane

`e2e/` : des scénarios Playwright lancés par Bun (`./run.sh scenario.ts`), contre le **vrai backend
Rust** sur une racine jetable (un compte de test, aucun fichier d'appareil : rien sur le réseau n'est
touché), l'app web servie par le backend. Les états d'appareils qu'un e2e doit couvrir (une lampe
allumée, une prise injoignable) sont servis par Playwright (`page.route`) à la place de l'appareil.
axe-core pour l'accessibilité, mesures de mise en page à 390, 1024, 1440 et 2560 px.

## Pièges (résumé)

1. Kit 3 : `#lib/…` avec l'extension, jamais `$lib`.
2. Kit 3 : `%sveltekit.env.X%` et `$app/env/public` lisent `src/env.ts`, plus `process.env`.
3. TS 6, pas 7 (svelte-check).
4. `light-dark()` : couleurs seulement.
5. Vitest browser : `src/test-setup.ts` pour `$app/env/public` ; textes exacts par défaut.
