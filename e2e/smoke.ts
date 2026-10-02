// The account and the way around: any page asks for a passkey first, an invitation lets
// in, the three destinations, a reload that keeps the session, signing out and back in
// where one was (no name, no password).
import { BASE, USER, check, checkNoErrors, done, launch, open, signIn, title } from './lib.ts';

const browser = await launch();
const p = await open(browser);
const passkeyButton = p.getByRole('button', { name: 'Se connecter avec ma clé d’accès' });

// any page asks to sign in first, in place: one button, no field
await p.goto(`${BASE}/tempo-predictions`);
await passkeyButton.waitFor();
check('a page asks for a passkey first, no field to fill', (await p.locator('main input').count()) === 0);

// in by an invitation, as a first visit
await signIn(p);
check('in by an invitation', await p.getByRole('heading', { level: 1, name: 'Accueil' }).isVisible());

// the three destinations, by the header's navigation
const nav = p.getByRole('navigation', { name: 'Navigation principale' }).first();
for (const [link, h1] of [
	['Télécommande', /Télécommande/],
	['Accueil', 'Accueil'],
	['Tempo', /Tempo|Prévision/]
] as const) {
	await nav.getByRole('link', { name: link }).click();
	await title(p, h1);
	check(`« ${link} » is the current page`, (await nav.getByRole('link', { name: link }).getAttribute('aria-current')) === 'page');
	check(`focus on the new title after « ${link} »`, await p.evaluate(() => document.activeElement?.tagName === 'H1'));
}

// the session survives a reload (HttpOnly cookie)
await p.reload();
await title(p, /Tempo|Prévision/);
check('a reload keeps the session', true);

// the old /login bookmark goes home
await p.goto(`${BASE}/login`);
await title(p, 'Accueil');
check('/login leads home', new URL(p.url()).pathname === '/');

// signing out, from the session panel
await p.getByRole('button', { name: `Session de ${USER.name}` }).click();
await p.getByRole('button', { name: 'Déconnexion' }).click();
await passkeyButton.waitFor();
check('signed out: the passkey button again', true);
await p.reload();
const after = await Promise.race([
	passkeyButton.waitFor().then(() => 'signed out'),
	p.locator('main h1:not(:has-text("Bon retour"))').waitFor().then(() => 'signed in again')
]);
check('and still signed out after a reload', after === 'signed out', after);

// back in with the passkey, on a deep link: lands on that page
await p.goto(`${BASE}/tempo-predictions`);
await passkeyButton.click();
await title(p, /Tempo|Prévision/);
check('signed in again where asked, with the passkey alone', p.url().endsWith('/tempo-predictions'));

checkNoErrors();
await done(browser);
