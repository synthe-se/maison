// Passkeys in a real browser (Ariane's scenario): an invitation creates the first passkey,
// the same device cannot register twice, a second one is added, the last one cannot be
// removed, an admin invites someone who is no admin (an older path link still works), sign out
// and in again with the passkey, my sessions listed, an admin signs someone out everywhere.
// axe on each screen.
import type { Page } from 'playwright';
import {
	BASE,
	USER,
	audit,
	authenticator,
	check,
	checkNoErrors,
	done,
	fr,
	invitation,
	launch,
	open,
	settled,
	title,
	toast
} from './lib.ts';
const keyRows = (p: Page) => p.getByRole('region', { name: fr.keys_title() }).getByRole('listitem');

const link = invitation();
check('the CLI printed an invitation link, its token in the fragment', /\/invite#[A-Za-z0-9_-]{43}$/.test(link));
const browser = await launch();
// axe is injected inline: past the CSP, as in a11y.ts
const p = await open(browser, { viewport: { width: 1280, height: 900 }, bypassCSP: true });
const device = await authenticator(p);

// the invitation greets by name, creates the passkey, then a word before going in
await p.goto(link);
await p.getByRole('heading', { name: fr.invite_title({ name: USER.name }) }).waitFor();
check('the token is wiped from the address', new URL(p.url()).hash === '', p.url());
await audit(p, 'invitation');
await p.getByRole('button', { name: fr.invite_create() }).click();
await p.getByRole('heading', { name: fr.invite_done_title() }).waitFor();
await p.getByRole('button', { name: fr.invite_enter() }).click();
await title(p, fr.nav_home());
check('signed in after creating the passkey', true);
check(
	'the invitation is single use',
	(await p.evaluate(
		async (token) =>
			(
				await fetch('/api/invites/greet', {
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ token })
				})
			).status,
		link.split('#')[1]
	)) === 404
);

// my account: one passkey; the same device refuses a second (excludeCredentials)
await p.getByRole('button', { name: fr.session_menu({ name: USER.name }) }).click();
await p.getByRole('link', { name: fr.account_title() }).click();
await title(p, fr.account_title());
check('one passkey listed', (await keyRows(p).count()) === 1);
const sessionRows = p.getByRole('region', { name: fr.sessions_title() }).getByRole('listitem');
await sessionRows.first().waitFor();
check(
	'my session listed, this device marked',
	(await sessionRows.count()) === 1 && (await sessionRows.first().getByText(fr.sessions_this_device()).isVisible())
);
await p.getByRole('button', { name: fr.keys_add() }).click();
await toast(p, fr.code_passkey_exists());
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
await toast(p, fr.code_last_passkey());
check('the last passkey cannot be removed', (await keyRows(p).count()) === 1);

// an admin invites someone: a link to copy
await p.getByRole('button', { name: fr.invites_title() }).click();
await p.getByRole('dialog').getByLabel(fr.invites_name()).fill('Alex');
await p.getByRole('dialog').getByRole('button', { name: fr.invites_create() }).click();
await p.getByRole('dialog').waitFor({ state: 'detached' });
const alexLink = await p.getByRole('textbox', { name: fr.invites_link({ name: 'Alex' }) }).inputValue();
check('an invitation link for Alex', alexLink.startsWith(`${BASE}/invite#`));
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

// Alex, on another device, from the link as an older mail wrote it (the token in its path):
// it moves to the fragment, then goes; not an admin
const q = await open(browser);
await authenticator(q);
await q.goto(alexLink.replace('/invite#', '/invite/'));
await q.getByRole('heading', { name: fr.invite_title({ name: 'Alex' }) }).waitFor();
check('an older path link greets too, its token wiped', new URL(q.url()).pathname === '/invite' && new URL(q.url()).hash === '', q.url());
await q.getByRole('button', { name: fr.invite_create() }).click();
await q.getByRole('button', { name: fr.invite_enter() }).click();
await title(q, fr.nav_home());
check('Alex is in, not an admin', (await q.evaluate(async () => (await fetch('/api/invites')).status)) === 403);
await q.goto(`${BASE}/account`);
await title(q, fr.account_title());
check('Alex sees no invitations', (await q.getByRole('heading', { name: fr.invites_title() }).count()) === 0);
check('Alex sees nobody to remove', (await q.getByRole('heading', { name: fr.people_title() }).count()) === 0);

// Alex's phone is lost: the admin signs them out everywhere, from « Qui peut entrer »
await p.reload();
await title(p, fr.account_title());
await p.getByRole('button', { name: fr.people_end_sessions_label({ name: 'Alex' }) }).click();
await p
	.getByRole('alertdialog', { name: fr.people_end_sessions_title({ name: 'Alex' }) })
	.getByRole('button', { name: fr.people_end_sessions() })
	.click();
await toast(p, fr.people_sessions_ended({ name: 'Alex' }));
check(
	'Alex is signed out everywhere',
	(await q.evaluate(async () => (await fetch('/api/auth/verify', { method: 'POST' })).status)) === 401
);

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
	(await r.getByRole('button', { name: fr.remote_add_binding() }).count()) === 0 && (await r.getByText(fr.code_forbidden()).count()) > 0
);

checkNoErrors();
await done(browser);
