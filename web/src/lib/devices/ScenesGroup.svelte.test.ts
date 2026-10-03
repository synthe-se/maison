import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { scene, sourceRoutes } from '#lib/test/remote.ts';
import ScenesGroup from './ScenesGroup.svelte';

const route = vi.hoisted(() => ({ url: new URL('http://localhost/') }));
vi.mock('$app/state', () => ({ page: route }));
const goto = vi.hoisted(() => vi.fn(async () => {}));
vi.mock('$app/navigation', () => ({ goto }));

const leave = scene({
	id: 'je-pars',
	name: 'Je pars',
	icon: 'log-out',
	actions: [
		{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' },
		{ action: 'meross_power', device: 'p1', state: 'off' }
	]
});
const house = (scenes = [leave, scene()], more: Record<string, unknown> = {}) =>
	stubApi({ ...sourceRoutes(), '/scenes': { success: true, scenes }, ...more });

beforeEach(() => {
	session.adopt(leonard);
	route.url = new URL('http://localhost/');
	goto.mockClear();
});
afterEach(() => {
	ui.toasts = [];
});

describe('ScenesGroup', () => {
	it('one button per scene; running one keeps the focus, says it is busy, then how it went', async () => {
		const answer = deferred();
		const api = house(undefined, { 'POST /scenes/je-pars/run': () => answer.promise });
		await render(ScenesGroup);
		const run = page.getByRole('button', { name: 'Je pars', exact: true });
		await run.click();
		await expect.element(run).toHaveAttribute('aria-busy', 'true');
		await expect.element(run).toHaveFocus();
		answer.resolve({ success: true, message: '2/2 actions ok', results: ['ok: a', 'ok: b'] });
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.scenes_done({ name: 'Je pars', count: 2 }));
		expect(api.sent('POST', '/scenes/je-pars/run')).toHaveLength(1);
	});

	it('a failed action is named, in a warning that stays', async () => {
		house(undefined, { 'POST /scenes/nuit/run': { success: false, results: ['failed: Suspension timed out'] } });
		await render(ScenesGroup);
		await page.getByRole('button', { name: 'Nuit', exact: true }).click();
		await expect
			.poll(() => ui.toasts.find((t) => t.warn)?.text)
			.toBe(m.scenes_failed({ name: 'Nuit', count: 1, total: 1, details: 'Suspension timed out' }));
	});

	it('no scene: an admin is offered three templates from the house’s devices; a template opens prefilled', async () => {
		const api = house([], { 'PUT /scenes/nuit': { success: true, scene: scene() } });
		await render(ScenesGroup);
		await page.getByRole('button', { name: m.scenes_create({ name: m.scenes_template_night() }) }).click();
		await expect.element(page.getByRole('dialog', { name: m.scenes_new() })).toBeVisible();
		await expect.element(page.getByLabelText(m.common_name())).toHaveValue(m.scenes_template_night());
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect
			.poll(() => api.sent('PUT', '/scenes/nuit').map((c) => c.body))
			.toEqual([
				{
					name: m.scenes_template_night(),
					icon: 'moon',
					actions: [
						{ action: 'zigbee_power', lamp: 'zb-1', state: 'off' },
						{ action: 'hue_power', lamp: 'hue-1', state: 'off' },
						{ action: 'cover', cover: 's1', command: 'close' }
					]
				}
			]);
		await expect.element(page.getByRole('dialog')).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.scenes_create({ name: m.scenes_template_night() }) })).toHaveFocus();
	});

	it('a member sees no group without scenes, and cannot edit them', async () => {
		session.adopt(alex);
		const api = house([]);
		const { container, unmount } = await render(ScenesGroup);
		await expect.poll(() => api.calls.some((c) => c.path === '/scenes')).toBe(true);
		expect(container.querySelector('section')).toBeNull();
		await unmount();
		// a new visit, from scratch (the first one's answer may still be on its way)
		forgetAll();
		house();
		await render(ScenesGroup);
		await expect.element(page.getByRole('button', { name: 'Je pars', exact: true })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.scenes_manage() })).not.toBeInTheDocument();
	});

	it('an admin deletes a scene after confirming', async () => {
		const api = house(undefined, { 'DELETE /scenes/je-pars': { success: true, message: '' } });
		await render(ScenesGroup);
		await expect.element(page.getByRole('button', { name: m.scenes_edit({ name: 'Je pars' }) })).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.scenes_manage() }).click();
		await page.getByRole('button', { name: m.scenes_edit({ name: 'Je pars' }) }).click();
		await page.getByRole('button', { name: m.scenes_delete({ name: 'Je pars' }) }).click();
		await page
			.getByRole('alertdialog')
			.getByRole('button', { name: m.scenes_delete({ name: 'Je pars' }) })
			.click();
		await expect.poll(() => api.sent('DELETE', '/scenes/je-pars')).toHaveLength(1);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.scenes_deleted({ name: 'Je pars' }));
	});

	it('/?scene=je-pars asks first, never runs silently; « Lancer » runs it and forgets the address', async () => {
		route.url = new URL('http://localhost/?scene=je-pars');
		const api = house(undefined, { 'POST /scenes/je-pars/run': { success: true, results: ['ok: a', 'ok: b'] } });
		await render(ScenesGroup);
		const dialog = page.getByRole('alertdialog', { name: m.scenes_run_title({ name: 'Je pars' }) });
		await expect.element(dialog).toBeVisible();
		expect(api.sent('POST', '/scenes/je-pars/run')).toHaveLength(0);
		await dialog.getByRole('button', { name: m.scenes_run({ name: 'Je pars' }) }).click();
		await expect.poll(() => api.sent('POST', '/scenes/je-pars/run')).toHaveLength(1);
		await expect.poll(() => goto.mock.calls.length).toBe(1);
		expect(goto).toHaveBeenCalledWith('/', { replace: true });
	});

	it('/?scene=: « Garder » runs nothing, and the address forgets it too', async () => {
		route.url = new URL('http://localhost/?scene=je-pars');
		const api = house();
		await render(ScenesGroup);
		const dialog = page.getByRole('alertdialog', { name: m.scenes_run_title({ name: 'Je pars' }) });
		await dialog.getByRole('button', { name: m.device_keep() }).click();
		await expect.poll(() => goto.mock.calls.length).toBe(1);
		expect(api.calls.filter((c) => c.method === 'POST')).toHaveLength(0);
	});

	it('/?scene= of a scene that does not exist: said, nothing run', async () => {
		route.url = new URL('http://localhost/?scene=nope');
		house();
		await render(ScenesGroup);
		await expect.poll(() => ui.toasts.find((t) => t.warn)?.text).toBe(m.scenes_not_found({ id: 'nope' }));
		await expect.element(page.getByRole('alertdialog')).not.toBeInTheDocument();
	});

	it('the editor says what is missing under the name, and keeps the scene unsaved', async () => {
		const api = house();
		await render(ScenesGroup);
		await page.getByRole('button', { name: m.scenes_new() }).click();
		await page.getByRole('button', { name: m.common_save() }).click();
		const name = page.getByLabelText(m.common_name());
		await expect.element(name).toHaveFocus();
		await expect.element(name).toHaveAccessibleDescription(m.scenes_missing_name());
		await userEvent.fill(name.element(), 'Matin');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(page.getByText(m.remote_missing_actions()).last()).toBeVisible();
		expect(api.calls.filter((c) => c.method === 'PUT')).toHaveLength(0);
	});
});
