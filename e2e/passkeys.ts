// Passkeys in a real browser (Ariane's scenario): an invitation creates the first passkey,
// the same device cannot register twice, a second one is added, the last one cannot be
// removed, an admin invites someone who is no admin, sign out and in again with the passkey.
// axe on each screen.
import type { Page } from 'playwright';
import { BASE, USER, authenticator, axe, check, checkNoErrors, done, fr, invitation, launch, open, settled, title, toast } from './lib.ts';

async function audit(p: Page, label: string) {
	const v = (await axe(p, ['wcag2a', 'wcag2aa', 'wcag21aa', 'wcag22aa'])).map((x) => `${x.id} ${x.nodes[0]?.target}`);
	check(`${label}: no axe violations`, v.length === 0);
	if (v.length) console.log(v);
}
const keyRows = (p: Page) => p.getByRole('region', { name: fr.keys_title() }).getByRole('listitem');

const link = invitation();
check('the CLI printed an invitation link', /\/invite\/[0-9a-f]{64}$/.test(link));
const browser = await launch();
// axe is injected inline: past the CSP, as in a11y.ts
const p = await open(browser, { viewport: { width: 1280, height: 900 }, bypassCSP: true });
const device = await authenticator(p);

// the invitation greets by name, creates the passkey, then a word before going in
await p.goto(link);
await p.getByRole('heading', { name: fr.invite_title({ name: USER.name }) }).waitFor();
await audit(p, 'invitation');
await p.getByRole('button', { name: fr.invite_create() }).click();
await p.getByRole('heading', { name: fr.invite_done_title() }).waitFor();
await p.getByRole('button', { name: fr.invite_enter() }).click();
await title(p, fr.nav_home());
check('signed in after creating the passkey', true);
check(
	'the invitation is single use',
	(await p.evaluate(async (u) => (await fetch(`/api/invites/${u.split('/invite/')[1]}`)).status, link)) === 404
);

// my account: one passkey; the same device refuses a second (excludeCredentials)
await p.getByRole('button', { name: fr.session_menu({ name: USER.name }) }).click();
await p.getByRole('link', { name: fr.account_title() }).click();
await title(p, fr.account_title());
check('one passkey listed', (await keyRows(p).count()) === 1);
await p.getByRole('button', { name: fr.keys_add() }).click();
await toast(p, fr.pk_exists());
check('the same device cannot register twice, said plainly', true);
// another device: a second authenticator, the first one put away
await device.cdp.send('WebAuthn.removeVirtualAuthenticator', { authenticatorId: device.authenticatorId });
await device.add();
await p.getByRole('button', { name: fr.keys_add() }).click();
await toast(p, fr.keys_added());
await p.waitForFunction(() => document.querySelectorAll('[aria-labelledby="keys-title"] li').length === 2);
check('a second passkey', (await keyRows(p).count()) === 2);
// removing asks first; once removed, the focus is on the list's title, not lost
const removeKey = async () => {
	await p
		.getByRole('button', { name: fr.keys_remove_label({ name: '' }).trim() })
		.first()
		.click();
	await p.getByRole('alertdialog').getByRole('button', { name: fr.keys_remove_action() }).click();
};
await removeKey();
await toast(p, fr.keys_removed());
await p.waitForFunction(() => document.activeElement?.id === 'keys-title');
check('after removing a passkey, the focus is on its list’s title', await p.evaluate(() => document.activeElement?.id === 'keys-title'));
await removeKey();
await toast(p, fr.pk_last());
check('the last passkey cannot be removed', (await keyRows(p).count()) === 1);

// an admin invites someone: a link to copy
await p.getByRole('button', { name: fr.invites_title() }).click();
await p.getByRole('dialog').getByLabel(fr.invites_name()).fill('Alex');
await p.getByRole('dialog').getByRole('button', { name: fr.invites_create() }).click();
await p.getByRole('dialog').waitFor({ state: 'detached' });
const alexLink = await p.getByRole('textbox', { name: fr.invites_link({ name: 'Alex' }) }).inputValue();
check('an invitation link for Alex', alexLink.startsWith(`${BASE}/invite/`));
check('listed as pending', await p.getByRole('button', { name: fr.invites_revoke_label({ name: 'Alex' }) }).isVisible());
await audit(p, 'account');

// sign out, then in with the passkey: no name, no password
await p.getByRole('button', { name: fr.session_menu({ name: USER.name }) }).click();
await p.getByRole('button', { name: fr.auth_logout() }).click();
await p.getByRole('heading', { name: fr.pk_signin_title() }).waitFor();
await audit(p, 'sign-in');
check('no field on the sign-in page, one button', (await p.locator('main input').count()) === 0);
await p.getByRole('button', { name: fr.pk_button() }).click();
await title(p, fr.account_title());
check('signed in again with the passkey, on the page left', true);

// Alex, on another device, from the link: not an admin
const q = await open(browser);
await authenticator(q);
await q.goto(alexLink);
await q.getByRole('button', { name: fr.invite_create() }).click();
await q.getByRole('button', { name: fr.invite_enter() }).click();
await title(q, fr.nav_home());
check('Alex is in, not an admin', (await q.evaluate(async () => (await fetch('/api/invites')).status)) === 403);
await q.goto(`${BASE}/account`);
await title(q, fr.account_title());
check('Alex sees no invitations', (await q.getByRole('heading', { name: fr.invites_title() }).count()) === 0);
check('Alex sees nobody to remove', (await q.getByRole('heading', { name: fr.people_title() }).count()) === 0);

// a member made by the CLI (as on the Pi): the dashboard without its admin settings
const r = await open(browser);
await authenticator(r);
await r.goto(invitation('sam', 'Sam', false));
await r.getByRole('button', { name: fr.invite_create() }).click();
await r.getByRole('button', { name: fr.invite_enter() }).click();
await title(r, fr.nav_home());
await r.getByRole('heading', { name: fr.shutters_title(), level: 2 }).waitFor();
await settled(r);
check('a member is offered no shutter to add', (await r.getByRole('button', { name: fr.shutters_add() }).count()) === 0);
check('a member cannot add lamps', (await r.getByRole('button', { name: fr.lamps_add() }).count()) === 0);
await r.goto(`${BASE}/tv`);
await r.locator('main h1').first().waitFor();
await settled(r);
check(
	'a member sees no TV settings',
	(await r.getByRole('button', { name: fr.common_settings_of({ name: fr.tv_default_name() }) }).count()) === 0
);
await r.goto(`${BASE}/remote`);
await title(r, fr.remote_title());
check(
	'a member configures no remote key',
	(await r.getByRole('button', { name: fr.remote_add_binding() }).count()) === 0 && (await r.getByText(fr.admin_only()).count()) > 0
);

checkNoErrors();
await done(browser);
