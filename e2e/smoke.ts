// The account and the way around: signing in (refused, then accepted), the three
// destinations, a reload that keeps the session, signing out.
import { BASE, USER, check, checkNoErrors, done, launch, open, signIn, title } from './lib.ts';

const browser = await launch();
const p = await open(browser);

// any page asks to sign in first, in place
await p.goto(`${BASE}/tempo-predictions`);
await p.getByLabel('Nom d’utilisateur').fill(USER.name);
await p.getByLabel('Mot de passe').fill('wrong');
await p.getByRole('button', { name: 'Se connecter' }).click();
await p.getByRole('alert').filter({ hasText: /incorrect|Invalid/i }).first().waitFor();
check('a wrong password is refused, said in the form', await p.getByLabel('Mot de passe').isVisible());

// signing in on a deep link lands on that page, not on the home page
await p.getByLabel('Mot de passe').fill(USER.password);
await p.getByRole('button', { name: 'Se connecter' }).click();
await title(p, /Tempo|Prévision/);
check('signed in where asked', p.url().endsWith('/tempo-predictions'));

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
await p.getByRole('button', { name: 'Se connecter' }).waitFor();
check('signed out: the sign-in form again', true);
await p.reload();
const after = await Promise.race([
	p.getByRole('button', { name: 'Se connecter' }).waitFor().then(() => 'signed out'),
	p.locator('main h1').waitFor().then(() => 'signed in again')
]);
check('and still signed out after a reload', after === 'signed out', after);

// sign in again through the shared helper (it is what other scenarios use)
await signIn(p);
check('signIn() helper works', await p.locator('main h1').isVisible());

checkNoErrors();
await done(browser);
