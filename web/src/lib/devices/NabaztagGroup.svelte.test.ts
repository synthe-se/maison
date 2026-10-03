import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import NabaztagGroup from './NabaztagGroup.svelte';

const status = (reachable: boolean, tempoEnabled = true, host: string | null = '192.168.1.50') => ({
	success: true,
	config: { host, tempoEnabled },
	reachable
});
const tile = () => page.getByRole('article', { name: m.nabaztag_rabbit() });
const push = () => page.getByRole('button', { name: m.nabaztag_push_tempo() });

describe('NabaztagGroup', () => {
	afterEach(() => {
		ui.toasts = [];
	});

	it('says the rabbit is reachable and shows Tempo', async () => {
		stubApi({ '/nabaztag': status(true) });
		await render(NabaztagGroup);
		await expect.element(tile()).toMatchTextContent(m.nabaztag_reachable_synced());
		await expect.element(push()).toBeEnabled();
	});

	it('plays its tai chi, the same choreography as the remote key', async () => {
		const api = stubApi({ '/nabaztag': status(true), 'POST /nabaztag/ctl': { success: true, message: 'ok' } });
		await render(NabaztagGroup);
		await page.getByRole('button', { name: m.nabaztag_taichi() }).click();
		await expect
			.poll(() => api.sent('POST', '/nabaztag/ctl').map((c) => c.body))
			.toEqual([{ command: 'chor /vl/config/chor/taichi.chor' }]);
		await expect.poll(() => ui.toasts.map((t) => t.text)).toContain(m.nabaztag_taichi_done());
	});

	it('cannot dance when it is unreachable', async () => {
		stubApi({ '/nabaztag': status(false) });
		await render(NabaztagGroup);
		await expect.element(page.getByRole('button', { name: m.nabaztag_taichi() })).toBeDisabled();
	});

	it('says reachable alone when Tempo is off', async () => {
		stubApi({ '/nabaztag': status(true, false) });
		await render(NabaztagGroup);
		await expect.element(tile()).toMatchTextContent(m.nabaztag_reachable());
	});

	it('says it is unreachable, and cannot push then', async () => {
		stubApi({ '/nabaztag': status(false) });
		await render(NabaztagGroup);
		await expect.element(tile()).toMatchTextContent(m.state_unreachable());
		await expect.element(push()).toBeDisabled();
	});

	it('is not shown when no rabbit is configured', async () => {
		const api = stubApi({ '/nabaztag': status(false, false, null) });
		const { container } = await render(NabaztagGroup);
		await expect.poll(() => api.calls).toHaveLength(1);
		await expect.poll(() => container.textContent?.trim()).toBe('');
	});

	it('keeps its place while first asked', async () => {
		stubApi({ '/nabaztag': () => new Promise(() => {}) });
		await render(NabaztagGroup);
		await expect.element(page.getByRole('heading', { name: m.nabaztag_name() })).toBeVisible();
	});

	it('a server that does not answer: said in place, with a retry', async () => {
		stubApi({ '/nabaztag': new Response('{"error":"down"}', { status: 500 }) });
		await render(NabaztagGroup);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).toBeVisible();
	});

	it('pushes today’s color again, busy meanwhile, then says so and reads the rabbit', async () => {
		const answer = deferred();
		const api = stubApi({ '/nabaztag': status(true), 'POST /nabaztag/tempo/push': () => answer.promise });
		await render(NabaztagGroup);
		await push().click();
		await expect.element(push()).toHaveAttribute('aria-busy', 'true');
		await expect.element(push()).toBeDisabled();
		answer.resolve({ success: true });
		await expect.element(push()).toBeEnabled();
		expect(api.sent('POST', '/nabaztag/tempo/push')[0].body).toEqual({ forceRefresh: false });
		expect(ui.toasts.map((t) => t.text)).toContain(m.nabaztag_tempo_pushed());
		expect(api.sent('GET', '/nabaztag')).toHaveLength(2);
	});

	it('tells a failed push', async () => {
		stubApi({ '/nabaztag': status(true), 'POST /nabaztag/tempo/push': new Response('{"error":"timeout"}', { status: 504 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await render(NabaztagGroup);
		await push().click();
		await expect.poll(() => fail).toHaveBeenCalledOnce();
	});
});
