// Passkeys in a real browser (Ariane's scenario): an invitation creates the first passkey,
// the same device cannot register twice, a second one is added, the last one cannot be
// removed, an admin invites someone who is no admin, sign out and in again with the passkey.
// axe on each screen.
import type { Page } from 'playwright';
import { BASE, USER, authenticator, axe, check, checkNoErrors, done, invitation, launch, open, title, toast } from './lib.ts';

async function audit(p: Page, label: string) {
	const v = (await axe(p, ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'])).map((x) => `${x.id} ${x.nodes[0]?.target}`);
	check(`${label}: no axe violations`, v.length === 0);
	if (v.length) console.log(v);
}
const keyRows = (p: Page) => p.getByRole('region', { name: 'Mes clés d’accès' }).getByRole('listitem');

const link = invitation();
check('the CLI printed an invitation link', /\/invite\/[0-9a-f]{64}$/.test(link));
const browser = await launch();
// axe is injected inline: past the CSP, as in a11y.ts
const p = await open(browser, { viewport: { width: 1280, height: 900 }, bypassCSP: true });
const device = await authenticator(p);

// the invitation greets by name, creates the passkey, then a word before going in
await p.goto(link);
await p.getByRole('heading', { name: `Bienvenue, ${USER.name}` }).waitFor();
await audit(p, 'invitation');
await p.getByRole('button', { name: 'Créer ma clé d’accès' }).click();
await p.getByRole('heading', { name: 'Ta clé d’accès est prête' }).waitFor();
await p.getByRole('button', { name: 'Entrer dans Maison' }).click();
await title(p, 'Accueil');
check('signed in after creating the passkey', true);
check('the invitation is single use', (await p.evaluate(async (u) => (await fetch(`/api/invites/${u.split('/invite/')[1]}`)).status, link)) === 404);

// my account: one passkey; the same device refuses a second (excludeCredentials)
await p.getByRole('button', { name: `Session de ${USER.name}` }).click();
await p.getByRole('link', { name: 'Mon compte' }).click();
await title(p, 'Mon compte');
check('one passkey listed', (await keyRows(p).count()) === 1);
await p.getByRole('button', { name: 'Ajouter une clé d’accès' }).click();
await toast(p, 'Cette clé est déjà enregistrée ici.');
check('the same device cannot register twice, said plainly', true);
// another device: a second authenticator, the first one put away
await device.cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId: device.authenticatorId });
await device.add();
await p.getByRole('button', { name: 'Ajouter une clé d’accès' }).click();
await toast(p, 'Clé ajoutée.');
await p.waitForFunction(() => document.querySelectorAll('[aria-labelledby="keys-title"] li').length === 2);
check('a second passkey', (await keyRows(p).count()) === 2);
// removing asks first; once removed, the focus is on the list's title, not lost
const removeKey = async () => {
	await p.getByRole('button', { name: /Retirer la clé/ }).first().click();
	await p.getByRole('alertdialog').getByRole('button', { name: 'Retirer la clé' }).click();
};
await removeKey();
await toast(p, 'Clé retirée.');
await p.waitForFunction(() => document.activeElement?.id === 'keys-title');
check('after removing a passkey, the focus is on its list’s title', await p.evaluate(() => document.activeElement?.id === 'keys-title'));
await removeKey();
await toast(p, /C’est ta dernière clé/);
check('the last passkey cannot be removed', (await keyRows(p).count()) === 1);

// an admin invites someone: a link to copy
await p.getByRole('button', { name: 'Inviter quelqu’un' }).click();
await p.getByRole('dialog').getByLabel('Prénom').fill('Alex');
await p.getByRole('dialog').getByRole('button', { name: 'Créer le lien' }).click();
await p.getByRole('dialog').waitFor({ state: 'detached' });
const alexLink = await p.getByRole('textbox', { name: 'Le lien pour Alex' }).inputValue();
check('an invitation link for Alex', alexLink.startsWith(`${BASE}/invite/`));
check('listed as pending', await p.getByRole('button', { name: 'Annuler l’invitation de Alex' }).isVisible());
await audit(p, 'account');

// sign out, then in with the passkey: no name, no password
await p.getByRole('button', { name: `Session de ${USER.name}` }).click();
await p.getByRole('button', { name: 'Déconnexion' }).click();
await p.getByRole('heading', { name: 'Bon retour' }).waitFor();
await audit(p, 'sign-in');
check('no field on the sign-in page, one button', (await p.locator('main input').count()) === 0);
await p.getByRole('button', { name: 'Se connecter avec ma clé d’accès' }).click();
await title(p, 'Mon compte');
check('signed in again with the passkey, on the page left', true);

// Alex, on another device, from the link: not an admin
const q = await open(browser);
await authenticator(q);
await q.goto(alexLink);
await q.getByRole('button', { name: 'Créer ma clé d’accès' }).click();
await q.getByRole('button', { name: 'Entrer dans Maison' }).click();
await title(q, 'Accueil');
check('Alex is in, not an admin', (await q.evaluate(async () => (await fetch('/api/invites')).status)) === 403);
await q.goto(`${BASE}/account`);
await title(q, 'Mon compte');
check('Alex sees no invitations', (await q.getByRole('heading', { name: 'Inviter quelqu’un' }).count()) === 0);
check('Alex sees nobody to remove', (await q.getByRole('heading', { name: 'Qui peut entrer' }).count()) === 0);

// a member made by the CLI (as on the Pi): the dashboard without its admin settings
const r = await open(browser);
await authenticator(r);
await r.goto(invitation('sam', 'Sam', false));
await r.getByRole('button', { name: 'Créer ma clé d’accès' }).click();
await r.getByRole('button', { name: 'Entrer dans Maison' }).click();
await title(r, 'Accueil');
await r.getByRole('heading', { name: 'Volets', level: 2 }).waitFor();
await r.waitForTimeout(800);
check('a member is offered no shutter to add', (await r.getByRole('button', { name: 'Ajouter', exact: true }).count()) === 0);
check('a member sees no TV settings', (await r.getByRole('button', { name: /^Réglages de (TV|Box|LEAP)/ }).count()) === 0);
check('a member cannot pair lamps', (await r.getByRole('button', { name: 'Appairer', exact: true }).count()) === 0);
await r.goto(`${BASE}/remote`);
await title(r, 'Télécommande');
check('a member configures no remote key', (await r.getByRole('button', { name: 'Ajouter une touche' }).count()) === 0 && (await r.getByText('Réservé aux administrateurs de Maison.').count()) > 0);

checkNoErrors();
await done(browser);
