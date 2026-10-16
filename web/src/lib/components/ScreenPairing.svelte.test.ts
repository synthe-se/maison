import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import ScreenPairing from './ScreenPairing.svelte';

/** A pairing as the backend starts it, asked about every 20 ms in a test. */
const pairing = (code = 'ABCD-EFGH', secret = 's1') => ({ success: true, code, secret, expiresInMs: 600_000, pollMs: 20 });
const waiting = { success: true, paired: false };

describe('ScreenPairing', () => {
	afterEach(() => {
		session.status = 'loading';
		session.screen = null;
	});

	it('shows its code big, spells it for a reader, and says how to give it', async () => {
		stubApi({ 'POST /screens/pairing': pairing(), 'POST /screens/pairing/claim': waiting });
		await render(ScreenPairing);
		await expect.element(page.getByRole('heading', { level: 1, name: m.screen_pair_title() })).toBeVisible();
		await expect.element(page.getByText(m.screen_pair_intro())).toBeVisible();
		await expect.element(page.getByText('ABCD-EFGH')).toBeVisible();
		await expect.element(page.getByText(`${m.screen_pair_code()} A B C D - E F G H`)).toBeInTheDocument();
		await expect.element(page.getByText(m.screen_pair_valid({ minutes: 10 }))).toBeVisible();
	});

	it('asks with its secret until an admin pairs it, then opens as that screen', async () => {
		const backend = stubApi({ 'POST /screens/pairing': pairing(), 'POST /screens/pairing/claim': waiting });
		const say = vi.spyOn(ui, 'say');
		await render(ScreenPairing);
		await expect.poll(() => backend.sent('POST', '/screens/pairing/claim').length).toBeGreaterThan(1);
		expect(backend.sent('POST', '/screens/pairing/claim')[0].body).toEqual({ secret: 's1' });
		expect(session.status).toBe('loading');
		backend.routes['POST /screens/pairing/claim'] = { success: true, paired: true, screen: { name: 'Salon' } };
		await expect.poll(() => session.screen).toBe('Salon');
		expect(session.status).toBe('signed_in');
		expect(say).toHaveBeenCalledWith(m.screen_paired({ name: 'Salon' }));
	});

	it('a code past its time offers a new one, the focus on it', async () => {
		const backend = stubApi({
			'POST /screens/pairing': pairing(),
			'POST /screens/pairing/claim': new Response(JSON.stringify({ success: false, code: 'pairing_unknown' }), { status: 404 })
		});
		await render(ScreenPairing);
		const again = page.getByRole('button', { name: m.screen_pair_again() });
		await expect.element(page.getByText(m.screen_pair_expired())).toBeVisible();
		await expect.element(again).toHaveFocus();
		backend.routes['POST /screens/pairing'] = pairing('JKLM-NPQR', 's2');
		backend.routes['POST /screens/pairing/claim'] = waiting;
		await again.click();
		await expect.element(page.getByText('JKLM-NPQR')).toBeVisible();
		await expect.poll(() => backend.sent('POST', '/screens/pairing/claim').at(-1)?.body).toEqual({ secret: 's2' });
	});

	it('stops asking once it is left', async () => {
		const backend = stubApi({ 'POST /screens/pairing': pairing(), 'POST /screens/pairing/claim': waiting });
		const view = await render(ScreenPairing);
		await expect.poll(() => backend.sent('POST', '/screens/pairing/claim').length).toBeGreaterThan(0);
		view.unmount();
		const asked = backend.sent('POST', '/screens/pairing/claim').length;
		await new Promise((resolve) => setTimeout(resolve, 100));
		expect(backend.sent('POST', '/screens/pairing/claim').length).toBe(asked);
	});
});
