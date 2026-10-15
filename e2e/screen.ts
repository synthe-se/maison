// A wall screen (the frame, frame/android and frame/linux): in by a service token given once in
// the address's fragment, its language beside it (the frame's WPE says en-US), the house's pages
// with nobody signed in and no account; its token revoked, it says so, and offers no passkey.
import { BASE, check, checkNoErrors, done, fr, launch, open, revokeToken, serviceToken, title } from './lib.ts';

const browser = await launch();
const p = await open(browser, { viewport: { width: 800, height: 1280 }, locale: 'en-US' });
const token = serviceToken('cadre');

await p.goto(`${BASE}/#token=${token}&lang=fr`);
await title(p, fr.nav_home());
check('the screen is in by its token, on the dashboard, in the language its fragment names', true);
check('the page says so', (await p.evaluate(() => document.documentElement.lang)) === 'fr');
check('the token is gone from the address', !p.url().includes('#') && !p.url().includes(token), p.url());
check('no passkey asked', (await p.getByRole('button', { name: fr.pk_button() }).count()) === 0);
check('nobody signed in: no session menu', (await p.locator('header button').count()) === 0);

// its pages, and a reload without the fragment: still the screen
await p.getByRole('link', { name: fr.nav_tempo() }).first().click();
await title(p, fr.nav_tempo());
await p.reload();
await title(p, fr.nav_tempo());
check('a reload keeps the screen in (the token kept, not in the address)', new URL(p.url()).hash === '');

// revoked on the Pi: the next request says so
revokeToken('cadre');
await p.goto(`${BASE}/`);
await title(p, fr.screen_revoked_title());
check('a revoked screen says so', await p.getByText(fr.screen_revoked_body()).isVisible());
check('and offers no passkey', (await p.getByRole('button', { name: fr.pk_button() }).count()) === 0);

checkNoErrors();
await done(browser);
