import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { IrBinding } from '#lib/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { stubApi } from '#lib/test/api.ts';
import { remoteSources, sourceRoutes } from '#lib/test/remote.ts';
import { summarize } from '#lib/devices/remote/actions.ts';
import { keyName } from '#lib/devices/remote/keys.ts';
import RemotePage from './+page.svelte';

const keymap: Record<string, IrBinding> = {
	'353': { label: 'Taichi', repeat: true, actions: [{ action: 'nabaztag', command: 'chor taichi' }, { action: 'zigbee_power', lamp: 'zb-1', state: 'toggle' }] },
	'115': { actions: [{ action: 'meross_power', device: 'p1', state: 'on' }] }
};
const site = (map: Record<string, IrBinding> = keymap, more: Record<string, unknown> = {}) =>
	stubApi({ ...sourceRoutes(), '/ir/keymap': { success: true, keymap: map }, '/ir/recent': () => new Promise(() => {}), ...more });
const bindings = () => page.getByRole('region', { name: m.remote_bindings_title() }).getByRole('heading', { level: 3 });

describe('remote page', () => {
	beforeEach(() => session.adopt(leonard));
	afterEach(() => {
		forgetAll();
		ui.toasts = [];
	});

	it('lists the configured buttons by key, each with its actions in words', async () => {
		site();
		await render(RemotePage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.remote_title() })).toBeVisible();
		await expect.element(bindings()).toHaveLength(2);
		await expect.element(bindings().nth(0)).toHaveTextContent(keyName(115));
		await expect.element(bindings().nth(1)).toHaveTextContent('Taichi');
		await expect.element(page.getByText(m.remote_binding_count({ count: 2 }))).toBeVisible();
		await expect.element(page.getByText(summarize(keymap['353'].actions[1], remoteSources()))).toBeVisible();
		await expect.element(page.getByText(summarize(keymap['115'].actions[0], remoteSources()))).toBeVisible();
		await expect.element(page.getByText(m.remote_repeat(), { exact: true })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.remote_key_mapped({ key: 'OK', label: 'Taichi' }) })).toBeVisible();
	});

	it('is titled the Maison way (« Télécommande · Maison »), with one h1', async () => {
		site();
		await render(RemotePage);
		await expect.poll(() => document.title).toBe(`${m.remote_title()} · ${m.branding_name()}`);
	});

	it('a member sees what each button does, but configures nothing', async () => {
		session.adopt(alex);
		site();
		await render(RemotePage);
		await expect.element(bindings()).toHaveLength(2);
		await expect.element(page.getByText(m.admin_only())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.remote_add_binding() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.remote_edit_binding({ key: keyName(115) }) })).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.remote_key_mapped({ key: 'OK', label: 'Taichi' }) })).not.toBeInTheDocument();
	});

	it('a binding edited and not saved is not lost to Escape: the sheet asks first', async () => {
		site();
		await render(RemotePage);
		await page.getByRole('button', { name: m.remote_edit_binding({ key: keyName(115) }) }).click();
		const sheet = page.getByRole('dialog', { name: m.remote_edit_binding({ key: keyName(115) }) });
		await sheet.getByLabelText(m.remote_label()).fill('Lampe');
		await (await import('vitest/browser')).userEvent.keyboard('{Escape}');
		await expect.element(page.getByRole('alertdialog', { name: m.sheet_discard_title() })).toBeVisible();
	});

	it('says when nothing is configured yet', async () => {
		site({});
		await render(RemotePage);
		await expect.element(page.getByText(m.remote_no_bindings())).toBeVisible();
		await expect.element(page.getByText(m.remote_no_bindings_hint())).toBeVisible();
	});

	it('says when the configuration cannot be read, and tries again', async () => {
		const api = site(keymap, { '/ir/keymap': new Response('{"error":"down"}', { status: 500 }) });
		await render(RemotePage);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		api.routes['/ir/keymap'] = { success: true, keymap };
		await page.getByRole('button', { name: m.common_retry() }).click();
		await expect.element(bindings()).toHaveLength(2);
	});

	it('opens a key’s binding from the picture or the list, in a sheet named after it', async () => {
		site();
		await render(RemotePage);
		await page.getByRole('button', { name: m.remote_key_mapped({ key: 'OK', label: 'Taichi' }) }).click();
		const sheet = page.getByRole('dialog', { name: m.remote_edit_binding({ key: 'OK' }) });
		await expect.element(sheet.getByLabelText(m.remote_label())).toHaveValue('Taichi');
		await sheet.getByRole('button', { name: m.common_cancel() }).click();
		await expect.element(sheet).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.remote_edit_binding({ key: keyName(115) }) }).click();
		await expect.element(page.getByRole('dialog', { name: m.remote_edit_binding({ key: keyName(115) }) })).toBeVisible();
	});

	it('adds a new button: the sheet captures, and saving reads the list again', async () => {
		const api = site({}, { 'PUT /ir/keymap/116': { success: true, message: '' } });
		await render(RemotePage);
		await page.getByRole('button', { name: m.remote_add_binding() }).click();
		const sheet = page.getByRole('dialog', { name: m.remote_new_key() });
		await expect.element(sheet.getByText(m.remote_capturing())).toBeVisible();
		await sheet.getByRole('textbox', { name: m.remote_key_code() }).fill('116');
		await sheet.getByRole('button', { name: m.remote_add_action() }).click();
		await sheet.getByRole('textbox', { name: m.remote_fields_command() }).fill('ping');
		api.routes['/ir/keymap'] = { success: true, keymap: { '116': { actions: [{ action: 'nabaztag', command: 'ping' }] } } };
		await sheet.getByRole('button', { name: m.common_save() }).click();
		await expect.element(sheet).not.toBeInTheDocument();
		await expect.element(bindings()).toHaveLength(1);
		await expect.element(bindings().nth(0)).toHaveTextContent(keyName(116));
	});

	it('deletes a button once confirmed, says so, and reads the list again', async () => {
		const api = site(keymap, { 'DELETE /ir/keymap/115': { success: true, message: '' } });
		const say = vi.spyOn(ui, 'say');
		await render(RemotePage);
		await page.getByRole('button', { name: m.remote_delete_key({ key: keyName(115) }) }).click();
		const dialog = page.getByRole('alertdialog', { name: m.remote_delete_title({ key: keyName(115) }) });
		await expect.element(dialog).toHaveAccessibleDescription(m.remote_delete_consequence());
		api.routes['/ir/keymap'] = { success: true, keymap: { '353': keymap['353'] } };
		await dialog.getByRole('button', { name: m.remote_delete_key({ key: keyName(115) }) }).click();
		await expect.poll(() => api.sent('DELETE', '/ir/keymap/115')).toHaveLength(1);
		await expect.poll(() => say).toHaveBeenCalledWith(m.remote_deleted({ key: keyName(115) }));
		await expect.element(bindings()).toHaveLength(1);
		await expect.element(dialog).not.toBeInTheDocument();
	});

	it('after a deletion the focus goes to the list it was in', async () => {
		const api = site(keymap, { 'DELETE /ir/keymap/115': { success: true, message: '' } });
		await render(RemotePage);
		await page.getByRole('button', { name: m.remote_delete_key({ key: keyName(115) }) }).click();
		api.routes['/ir/keymap'] = { success: true, keymap: { '353': keymap['353'] } };
		await page.getByRole('alertdialog').getByRole('button', { name: m.remote_delete_key({ key: keyName(115) }) }).click();
		await expect.element(bindings()).toHaveLength(1);
		await expect.element(page.getByRole('heading', { name: m.remote_bindings_title() })).toHaveFocus();
	});
});
