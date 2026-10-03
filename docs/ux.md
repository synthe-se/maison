# UX rules

The rules the web app follows, each one kept in code (the comments cite these sections).
Two people, mostly on a phone, sometimes a laptop or a 32-inch screen. Commands are
confirmed in ~100 ms (a Zigbee lamp) to tens of seconds (the TV out of deep standby ~20 s, a
shutter's ~30 s travel), and some devices never say their state (the IR air conditioner, the
TV once JointSPACE is down). Sources: Apple HIG, Material 3, WAI-ARIA APG, WCAG 2.2, NN/g,
Home Assistant's and Google Home's dashboards, read on 2 October 2026. WCAG 2.2 AA is a
floor.

## Layout

Ariane's layout and the synthe.se tokens (warm ground, ink, one petrol accent; Fraunces and
Cal Sans, self-hosted), in `web/src/styles/tokens.css`:

- Breakpoints: compact < 600, medium < 840, expanded < 1200, large < 1600, xl.
- One page container for every page and the header's inner row (`--page-max` 2400 px,
  margins 16 / 24 from 600 / 32 from 1600 px); content keeps its measure inside it
  (`--measure` 42rem), left-aligned. A spacing scale of 4 (`--s-1`…`--s-7`).
- One height scale for controls (`--control-h` 44, `-s` 36, `-xs` 32; all 44 on a coarse
  pointer; `-l` 52 for the door's one big action), one radius scale, one type scale (`--t-*`),
  one focus ring (`--focus-ring`); `web/scripts/css.check.ts` refuses literal radii, control
  heights, px font sizes, colors outside `tokens.css` and outline literals.
- Every color once, light and dark side by side in `light-dark()`; `data-theme` on `<html>`
  (set before first paint) forces a side.
- From 600 px a sticky 56 px header holds the destinations; below, a bottom bar does.
- Every page starts with `PageHead`: the title where the logo starts, actions on the right; the
  window's title is `pageTitle()` (« Salon · Maison »). Every group is `Group`: its title (an
  h2 the focus can go back to), one fact, its actions.
- Dashboard groups flow in columns of 22rem, six at most, each group whole, reading down a
  column then the next, 12 px between two groups of a column. A group's devices are one
  framed list of compact rows: 56 px a row (the 48 px gesture and 4 px above and below), a
  line between two, nothing sticky. `e2e/layout.ts` bounds the phone dashboard's height
  (the README house at 390 px) and checks the row height.
- The door (signing in, an invitation, « the house does not answer »): `AuthShell`, after
  Ariane's. Maison's identity on the brand's petrol, in both themes (the mark at 88 px, the
  name in Borel, the promise « Ta maison te répond, ici, sans passer par le nuage. », the
  sun's path the shutters follow drawn faintly, `aria-hidden`), the action on the other
  side: one column on a phone, two from 840 px. Each door page is a title, a lead and one
  52 px action; the focus goes to the page's h1 once in.
- No sideways scroll at 320 px (WCAG 1.4.10); `e2e/layout.ts` measures 320, 390, 1024, 1440
  and 2560 px.

## 1. Tiles

```
┌──────────────────────────────────────────────┐
│ [◉]  Lampe du salon                    40 %  │  gesture · name · one fact
│ 48px Allumée                                 │  the state in words
│ ───────────────●──────────────────────────── │  at most one control
└──────────────────────────────────────────────┘
```

- A row first: a control under it only when it can act. A lamp's brightness slider only
  under a lamp that is on and reachable; an off or unreachable lamp is one row, with no
  slider that looks draggable (its page says the same).
- The remotes keep their single controls on the dashboard (the TV: power and volume − / +;
  the box: wake/sleep and its app chips); their pads live on their pages (`/tv`,
  `/androidtv`).
- Line 2 says what matters before « En ligne »: a cat device says « Eau basse », « Litière
  à moitié pleine » (in the warning style) or « Propre »; « En ligne » only when there is
  nothing to say. A plug drawing power during a red day's peak hours says « Allumée en HP
  rouge · 0,76 €/h » in the warning style; its fact is the power and its cost at the price
  in force (« 42 W · 0,7 c€/h »).
- Nothing that looks like a status is a button: a Tuya device's local connection is a
  labelled switch on its page and in the group's ⋯ menu, never a Wi‑Fi icon on the tile.

- Two zones: the 48 px icon is the main gesture (on/off), the name and state lead to the
  device's page (`/device/[id]`, `/hue-lamp/[id]`, `/zigbee-lamp/[id]`, `/meross/[id]`): a
  page, not a modal, since details are long and a page keeps Back; `/tv`, `/androidtv`
  likewise). `DeviceTile` is the one tile.
- Line 2 says the state in words (« Allumée, 80 % », « Injoignable depuis 3 min »). No tile
  without a written state. The right-hand fact never wraps.
- On is a filled `--accent` disc, off an empty 1.5 px ring: two shapes, never color alone
  (WCAG 1.4.1, 1.4.11).
- A tile's settings unfold inside it under its gear (`DeviceTile` `settings`, `TileSettings`
  on a Bits UI Collapsible: `aria-expanded` and `aria-controls`); closed while the focus is in
  them (saved, cancelled), the focus goes back to the gear. A form that needs room is a
  `Sheet` instead (adding a shutter, an invitation, a meal, a remote key, a scene).

## 2. Commands

Three cases, three controls:

1. **State read from the device** (lamps, plugs, the TV while JointSPACE answers): the icon
   is a `<button aria-pressed>` named after the device, never « Allumer ». Not a switch
   outside a settings list.
2. **Settings on a device's page**: a list row with `role="switch"`, label on the left
   (`SettingRow`, `Toggle`).
3. **Assumed state** (IR air conditioner, TV over IR): two buttons, « Allumer » and
   « Éteindre », and the last order under them. A state nobody knows is not drawn.

A command in flight (`Command`, `#lib/command.svelte.ts`) is optimistic in the gesture, not
in the displayed state: the button shows the target at once; after 1 s a ring and
« Allumage… »; the device's answer wins; past the family's limit (lamps 3 s, plugs 5 s, TV
30 s, others 10 s) it returns to the read state with « Pas de réponse · Réessayer » on the
tile and one `role="status"` sentence. Over 10 s (waking the TV, sending an APK) a
determinate `Progress` bar on elapsed time.

Every other button that sends something goes through `Gesture` (`#lib/gesture.svelte.ts`):

- **A control that cannot act says why**: out of reach (a lamp, a shutter, the rabbit, the box's
  keys), offline (a litter box, a feeder), not now (econo outside cooling, an eleventh meal),
  it stays in place with `unavailable(reasonId)` (`#lib/gesture.svelte.ts`, next to
  `pending()`): `aria-disabled` and, as its description, the words that say why — the tile's
  state line (« Injoignable depuis 3 min »), an « offline » line with « Reconnecter », a hint.
  Never `disabled`: the focus would fall, and nothing would say why.
- **The focus never falls** (WCAG 2.4.3). A busy control is never `disabled`: it keeps the
  focus, carries `aria-busy`/`aria-disabled`, and a second press does nothing. When the
  pressed control disappears, the focus goes to the section's heading or the button that
  opened the form (`#lib/focus.ts`).
- **An error lives next to what it is about**: under the field (`aria-describedby`, focus back
  to it), on the tile, or as « can't read this right now » with a retry for a list never
  read, never an empty list that lies. The words are a refusal's own (`code`, `#lib/errors.ts`)
  or what to do; never « Erreur. » in front of server text.

## 3. Sliders

One slider, `Range` (APG slider through Bits UI): arrows one step, Page Up/Down a big step,
Home/End the bounds, the value spoken with its unit (`aria-valuetext`), a 44 px band that can
be touched anywhere (WCAG 2.5.7), `touch-action: pan-y`.

- Lamps and volume follow the finger, sending at most every 300 ms and on release; the thumb
  keeps the sent value until the device reports it (± a margin) or 3 s pass.
- A shutter's motor does not chase the finger: its target goes on release, or 400 ms after
  the last key.
- The slider sends through a gesture of its own: an order in flight on the same tile (Open,
  Close) never swallows a position; a refused one is said once (a toast) and the thumb goes back
  to what the device says.
- Shutters are an **open percentage** everywhere (0 closed, 100 open), labelled « Fermé » /
  « Ouvert » at the ends; Matter's inverted closure stays in `backend/src/matter/`.

## 4. Live state

- `live(source)` (`#lib/live.svelte.ts`): one shared value per key, polled per device while a
  view needs it, stopped when the page is hidden, read at once when it returns. Each source
  (its key, its one fetch, its pace) is declared once, in its family's `data.ts`: two views of
  a key ask the same way (live() refuses a key declared with two fetches). Cadences: 1 s for a
  moving shutter, 3–5 s lamps and plugs, 10–15 s Tuya, 60 s TV and box (never faster:
  JointSPACE dies under bursts), 120 s Nabaztag.
- An old value says so, once, where it is shown (`Loaded`, or a group's head: `Stale`):
  « Dernière lecture 14:20 · Réessayer », when the last ask failed or the polls have not
  answered for three of their intervals. A page the server says does not exist (404) says
  « introuvable »; one it could not read (no answer, a 5xx) says so with « Réessayer ».
- Unreachable: « Injoignable depuis 12 min », then the time it was last heard; the gesture
  stays visible but unavailable with its reason; the last value stays, muted.
- Nothing moves on refresh: `tabular-nums`, space reserved for the longest value, fixed order,
  skeletons at the final height. `e2e/layout.ts` checks no tile changes height.
- One `role="status"` region, present from load, only for the result of the viewer's own
  gesture. Never for a refresh or for what someone else did.

## 5. Groups and navigation

- « Maintenant » first: one row of chips summing up the house (the Tempo price in force
  until when, the lamps on, the next shutter move, what needs a hand, the devices out of
  reach). A summary that duplicates the groups and never reorders them: each chip leads to
  its group (scrolled, its title focused); what needs a hand in the warning style; two lines
  at most on a phone, the rest behind « +N » (the facts fold before the warnings).
- Then the scenes (« Je pars », « Nuit », « Film »: one button each; admins create, edit and
  delete them, starting from three templates filled from the house's devices; members see
  nothing until one exists).
- Then grouped by device type, not brand, radio or room (one room for now), in the owner's
  fixed order: Tempo, lamps (one group, Bluetooth and Zigbee alike; the radio is said on the
  lamp's page, « Ajouter une lampe » offers both), the cats' corner, air conditioning,
  shutters, plugs, the rabbit, then « Télé » (little used): the TV and the Android TV box in
  one group, each row leading to its page. The IR remote's keymap is not on the home: it is
  the « Télécommande » destination. The same order at every width; nothing reorders itself.
- Group actions where a group has several of a thing: « Tout éteindre » on the lamps (when
  one is on), « Tout ouvrir » / « Tout fermer » on the shutters. A shutter's next scheduled
  move can be skipped once next to it (« Fermeture 18:42 · Ne pas fermer ce soir », then
  « Fermeture de 18:42 sautée · Annuler »).
- Three destinations: Accueil, Tempo, Télécommande. Device pages are not destinations: the
  home tab stays current and the page starts with « Retour à l’accueil ».
- « Télécommande » is the IR keymap page only: the TV's and the box's pads live on their own
  pages (`/tv`, `/androidtv`), so one word never names two screens.

## 6. Long or irreversible actions

- Confirm only what cannot be undone (unpairing a shutter or lamp, resetting a counter,
  removing a passkey, ending every session): `ConfirmDialog` says the consequence, the button
  names the precise verb, the way back is « Garder », no default button, focus on the title.
- Everything else is immediate; a deleted meal can be restored from its toast (« Rétablir »,
  the toast's one action, 10 s once nothing holds it; the focus goes to it, then back to the
  section).
- A scene from the app's shortcuts (`/?scene=<id>`) is asked first (`ConfirmDialog`, its
  actions listed), never run silently; either answer clears the address.
- A group action or a scene says its outcome once: how many did it, or which ones did not
  answer and what they said, in a warning that stays.
- Giving portions or starting a clean: no dialog, the quantity in the label, the result on
  the tile.
- Long steps are named and timed (Matter pairing, APK upload then install); a form changed
  and not saved is not lost to a stray Escape (`Sheet`).

## 7. Pad and volume

- Pad: four 56 px arrows around a 64 px OK, Back/Home/Menu at 48 px under it; French
  accessible names (« Haut », « Valider »), never key codes.
- Hold to repeat: one step, then after 500 ms ten a second (volume five a second). Never for
  power, OK, Back, Home, mute or an AC key.
- Haptics fire on touch, not on the reply: a tap tick, a firmer one to commit, a double buzz
  on failure; silent where `navigator.vibrate` does not exist (iOS), where a 100 ms pressed
  state is the feedback.
- Keyboard shortcuts only while the pad has the focus (WCAG 2.1.4).

## 8. Tempo

- Color + shape + name, never color alone and never an initial (bleu and blanc start
  alike): blue filled, white an empty ring, red filled and hatched at 45° (`--tempo-red-hatch`;
  `devices/tempo/colors.ts`, `Swatch.svelte`; the code says blue/white/red, French only in the
  messages).
- Published by RTE: solid. A forecast: dashed outline and its probability (« Rouge probable ·
  62 % »); under 60 %, dotted and unwashed. Beyond tomorrow the forecast is Maison's, said
  unofficial, with its measured reliability ([tempo.md](tempo.md)).
- The current price follows the Tempo day (06:00 → 06:00) and the period (peak 06:00–22:00),
  recomputed on the spot at each switch (`devices/tempo/price.ts`, the one pricing module:
  the plugs' cost per hour and the month's estimate come from it too).
- On the dashboard, Tempo is one row, no « Aujourd’hui » header: the color and the price in
  force in c€/kWh until when (its link to the Tempo page), then tomorrow. Before 06:00 it says
  it as it is: « Encore bleu jusqu’à 06:00 », then « Rouge à partir de 06:00 ». The forecasts,
  the days left and every price (to the ten-thousandth of a euro) are on the Tempo page.
- A cost estimated from assumptions says so: the plug page's « Ce mois-ci ≈ 4,12 € » is
  labelled « estimation », its hint stating the split (an even draw over 24 h: 2/3 peak).
- The calendar is a plain `<table>` with a `<caption>`, abbreviated day headers with `abbr`,
  each cell read as « mardi 12 novembre, rouge », today `aria-current="date"`.

## 9. Copy

- Every text comes from `i18n/messages/{fr,en}.json` (Paraglide). `bun run check` refuses a
  key missing in a locale, unused, or saying the same as another (`web/scripts/i18n.check.ts`;
  legitimate homonyms in `i18n.allow.json`, with their reason).
- French addresses the person as « tu », with French typography: narrow no-break spaces
  before `: ; ? !` and inside « », typographic apostrophes, 24-hour times (« 20:24 »).
- Say what happened and what to do; name things the same way everywhere.

## Checks

- `e2e/a11y.ts`: axe (WCAG 2.2 AA) on every screen (the door's sign-in and invitation too),
  light and dark, desktop and phone.
- `e2e/layout.ts`: container, title alignment, bottom bar, no sideways scroll, stable tiles;
  the door at 320, 390 and 1440 px; the phone dashboard's height bound, 56 px rows, two
  lines of « Maintenant ».
- `e2e/dashboard.ts`: tiles, gestures, a device that does not answer, group actions, skipping
  a shutter move, scenes (made from a template, run, asked from a shortcut). The timings it
  checks (a command's 1 s and 3 s, a shutter's 400 ms) run on the page's clock
  (`page.clock`), never on a sleep; its words come from `fr.json`, its simulated house is typed
  with the app's types.
