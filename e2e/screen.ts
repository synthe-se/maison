// A wall screen (the frame, frame/android and frame/linux), paired as a TV signs in: signed out,
// its browser asks for a code; an admin, signed in on their phone, gives it from « Mon compte »;
// the screen opens by itself, its language and the sun's theme from its address (the frame's WPE
// says en-US and light), the house's pages with nobody signed in. No token in its address, its
// storage nor its scripts' cookies. Removed by the admin, it says so and offers to pair again, never a passkey.
import { BASE, check, checkNoErrors, done, fr, launch, open, signIn, title, toast } from './lib.ts';

const browser = await launch();
const screen = await open(browser, { viewport: { width: 800, height: 1280 }, locale: 'en-US' });
const phone = await open(browser);

// the admin, on their phone: the house's place is set (the shutters' sun schedule)
await signIn(phone, '/account');
await phone.evaluate(() =>
	fetch('/api/matter/place', {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name: 'Paris', latitude: 48.8566, longitude: 2.3522 })
	})
);
const side = await phone.evaluate(async () => (await (await fetch('/api/sun')).json()).side as string);

// the frame opens Maison as its kiosk does: its language and the sun's theme in the address
await screen.goto(`${BASE}/#lang=fr&theme=sun`);
await title(screen, fr.pk_signin_title());
check('the screen speaks the language its address names', (await screen.evaluate(() => document.documentElement.lang)) === 'fr');
await screen.waitForFunction((want) => document.documentElement.dataset.theme === want, side);
check(`signed out, the screen already follows the sun: ${side} now`, true);
await screen.getByRole('button', { name: fr.screen_pair_button() }).click();
const shown = screen.getByText(/^[A-Z2-9]{4}-[A-Z2-9]{4}$/);
await shown.waitFor();
const code = (await shown.textContent())!;
check('the screen shows a code to give', /^[A-Z2-9]{4}-[A-Z2-9]{4}$/.test(code), code);

// the admin, on their phone, in the house: « Mon compte », Écrans, Associer un écran
await phone.getByRole('button', { name: fr.screens_pair() }).click();
const sheet = phone.getByRole('dialog', { name: fr.screens_pair() });
await sheet.getByLabel(fr.screens_code()).fill(code.toLowerCase());
await sheet.getByLabel(fr.screens_name()).fill('Cadre');
await sheet.getByRole('button', { name: fr.screens_pair() }).click();
await toast(phone, fr.screens_paired({ name: 'Cadre' }));
check('the screen is listed', await phone.getByRole('region', { name: fr.screens_title() }).getByText('Cadre').isVisible());

// the screen opens by itself at its next question (every 3 s)
await title(screen, fr.nav_home());
check('the screen is in, on the dashboard, nobody signed in', (await screen.locator('header button').count()) === 0);
check('no passkey asked', (await screen.getByRole('button', { name: fr.pk_button() }).count()) === 0);
check('nothing in its address', new URL(screen.url()).hash === '' && !screen.url().includes('maison_'), screen.url());
const kept = await screen.evaluate(() => JSON.stringify({ ...localStorage }) + JSON.stringify({ ...sessionStorage }) + document.cookie);
check('no token a script could read: storage nor cookies', !kept.includes('maison_') && !kept.includes('screen'), kept);
const cookie = (await screen.context().cookies()).find((c) => c.name.endsWith('maison_screen'));
// its attributes only: a test never prints a secret
check(
	'its cookie is HttpOnly and SameSite=Lax',
	cookie?.httpOnly === true && cookie.sameSite === 'Lax',
	`httpOnly ${cookie?.httpOnly}, sameSite ${cookie?.sameSite}`
);

// signed in, it follows the sun from the house's place (and its hours)
await screen.reload();
await title(screen, fr.nav_home());
await screen.waitForFunction((want) => document.documentElement.dataset.theme === want, side);
check(`signed in, the screen follows the sun: ${side} now`, true);

await screen.getByRole('link', { name: fr.nav_tempo() }).first().click();
await title(screen, fr.nav_tempo());
await screen.reload();
await title(screen, fr.nav_tempo());
check('a reload keeps the screen in', true);

// removed by the admin: the screen's next request says so
await phone.reload();
await phone.getByRole('button', { name: fr.common_remove_named({ name: 'Cadre' }) }).click();
await phone
	.getByRole('alertdialog', { name: fr.screens_remove_title({ name: 'Cadre' }) })
	.getByRole('button', { name: fr.screens_remove_action() })
	.click();
await toast(phone, fr.screens_removed({ name: 'Cadre' }));
await screen.goto(`${BASE}/`);
await title(screen, fr.pk_signin_title());
check(
	'a removed screen opening again is signed out, its pairing offered',
	await screen.getByRole('button', { name: fr.screen_pair_button() }).isVisible()
);

checkNoErrors();
await done(browser);
