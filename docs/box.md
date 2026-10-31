# The Android TV box

A MECOOL LEAP-S1 (Google TV, Android 14, Amlogic) cleared of what it does not need, under a home of
our own. Both parts can go back: nothing leaves the system partition.

## The launcher (`box/launcher`)

Compose for TV, one APK, usable on any Android TV from Android 10. It hardcodes no app: a first home
takes the apps the person installed, then favorites are chosen on the box (hold OK: add, move,
remove, app info, uninstall through the system's own confirmation). Tabs Home and Apps, as Google's
launcher has them; Iris's type, palette and focus.

```bash
cd box/launcher
./gradlew testDebugUnitTest lintDebug
MAISON_BOX_SIGNING=~/maison-box/keys/release.properties ./gradlew assembleRelease
adb install -r app/build/outputs/apk/release/app-release.apk
```

The release key lives in `~/maison-box/keys/`. Becoming the home:
`adb shell cmd package set-home-activity maison.box/.Home`; Google's launcher keeps priority until
it is disabled (the cleanup below does both).

After three minutes without a key the home rests: everything goes but the time, which rises and
floats slowly over the picture; the first key only wakes it.

The day's picture is Wikimedia's featured picture in the box's language: landscape and credited in
a name and a license, else an earlier day's. Fetched by WorkManager once a day, on network; the
last one stays offline, the plain ground before any. Author and license are always shown.

Over ADB only (the receiver wants DUMP, the shell's alone):

```bash
adb shell am broadcast -n maison.box/.HouseReceiver --es favorites org.a,org.b
adb shell am broadcast -n maison.box/.HouseReceiver --el sunriseMs 0 --el sunsetMs 0 \
  --es side dark --es tempoToday BLUE --es tempoTomorrow WHITE
```

The second is what Maison sends every 30 min (`box_clean::push_house`); after two hours of silence
the home stops showing it. A home nobody talks to simply has no such line.

## The cleanup (`backend/src/box_clean.rs`)

One table, `GET`/`POST /api/androidtv/cleanup` (an admin's), the box's settings in the web app.

- `com.oemservice` is a remote-management agent running as the system user: MQTT, scripts, remote
  control, factory reset, logcat and usage upload, `READ_FRAME_BUFFER`, an accessibility service.
  Its broker did not answer: it retried every second, forever.
- Kept on purpose: the vendor's updates, `tv.remote.service` (Maison's keys), GMS, Play, WebView, Gboard, `setupwraith`,
  `rcupair` (the remote), `droidlogic`, TV settings, the screensaver.
- Also set by hand on this box: `wifi_scan_always_enabled` 0, `location_mode` 0 (it is on Ethernet).
- `tv.axel` stays: a Google TV component whose role is not documented.
- Without Cast, DIAL's wake (`dial_wake`, when ADB is down) no longer answers.
- `pm disable-user` is written to disk seconds later: a reboot right after loses it.
- `com.droidlogic` re-enables katniss, tvrecommendations and Google's speech at each boot: they
  are removed for user 0 instead (`cmd package install-existing --user 0 <package>` brings them back).
- The vendor's updates stay on (box and remote firmware): they do not need `com.oemservice`. After
  a firmware update, check the cleanup: one may bring packages back.

A full backup (every readable APK, settings, state) sits in `~/maison-box/backup-*`.

## Design sources

Read on 2026-10-31: [Android TV design](https://developer.android.com/design/ui/tv/guides/foundations/design-for-tv)
and [layouts](https://developer.android.com/design/ui/tv/guides/styles/layouts) (960x540 dp, 58/28 dp
margins), [focus](https://developer.android.com/design/ui/tv/guides/styles/focus-system),
[Compose for TV](https://developer.android.com/training/tv/playback/compose),
[TV accessibility](https://developer.android.com/training/tv/accessibility),
[WCAG 2.2 1.4.3](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) (text over a
picture: `Contrast.kt` proves 4.5:1 over pure white),
[1.4.11](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html),
[2.4.7](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html),
[Commons reuse](https://commons.wikimedia.org/wiki/Commons:Reusing_content_outside_Wikimedia),
[Wikimedia's User-Agent policy](https://foundation.wikimedia.org/wiki/Policy:User-Agent_policy).
