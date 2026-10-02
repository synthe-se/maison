import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { merossElectricity, merossStatus } from '#lib/test/meross.ts';
import { kwh, kwhFromWh, milliamps, volts, watts } from './units.ts';
import PlugDetail from './PlugDetail.svelte';

afterEach(() => forgetAll());

const consumption = {
	success: true,
	device: { id: 'p1', name: 'Radiateur' },
	consumption: Array.from({ length: 9 }, (_, i) => ({ date: `2026-09-${String(20 + i).padStart(2, '0')}`, time: 0, value: 1000 + i })),
	summary: { days: 9, totalWh: 9036, totalKwh: 9.036 },
	message: ''
};

/** Answers each of the page's three reads; records every call. */
function plugServer(status = merossStatus()) {
	return stubFetch((url) => {
		if (url === '/api/meross/p1/status') return json(status);
		if (url === '/api/meross/p1/electricity') return json(merossElectricity());
		if (url === '/api/meross/p1/consumption') return json(consumption);
		return json({ success: true });
	});
}

describe('PlugDetail', () => {
	it('reads status, electricity and consumption once each', async () => {
		const calls = plugServer();
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByRole('heading', { level: 1, name: 'Radiateur' })).toBeVisible();
		expect(calls.map((c) => c.url).sort()).toEqual(['/api/meross/p1/consumption', '/api/meross/p1/electricity', '/api/meross/p1/status']);
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Radiateur' })).not.toBeInTheDocument();
	});

	it('lists the last seven days, newest first, and the period’s total', async () => {
		plugServer();
		await render(PlugDetail, { id: 'p1' });
		const rows = page.getByRole('listitem');
		await expect.poll(() => rows.elements().length).toBe(7);
		expect(rows.elements()[0].textContent).toContain(kwhFromWh(1008));
		expect(rows.elements()[6].textContent).toContain(kwhFromWh(1002));
		await expect.element(page.getByText(`${m.meross_consumption_days({ count: 9 })} · ${kwh(9.036)}`)).toBeVisible();
	});

	it('names the hardware, unknown values in words', async () => {
		plugServer(merossStatus({ hardware: { type: 'mss310', version: '', chipType: 'mt7682', uuid: 'u', mac: 'aa' } }));
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByText('MSS310')).toBeVisible();
		await expect.element(page.getByText(m.common_unknown())).toBeVisible();
		await expect.element(page.getByText(m.meross_wifi_signal())).toBeVisible();
	});

	// the backend sends « 230.4V », « 0.183A », « 42.4W » (meross.rs format_*): read as numbers
	// they are NaN. The tile already reads `raw` (livePower); the page does not.
	it('says voltage, current and power in numbers', async () => {
		plugServer();
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByText(volts(230.4)), { timeout: 1000 }).toBeVisible();
		await expect.element(page.getByText(milliamps(0.183)), { timeout: 1000 }).toBeVisible();
		await expect.element(page.getByText(watts(42.4, 1)).first(), { timeout: 1000 }).toBeVisible();
	});

	it('the indicator light: two orders, each a POST to dnd', async () => {
		const calls = plugServer();
		const toast = vi.spyOn(ui, 'toast');
		await render(PlugDetail, { id: 'p1' });
		await page.getByRole('button', { name: m.meross_led_off() }).click();
		await expect.poll(() => toast.mock.calls.length).toBe(1);
		expect(toast).toHaveBeenCalledWith(m.meross_dnd_updated());
		await page.getByRole('button', { name: m.meross_led_on() }).click();
		await expect.poll(() => calls.filter((c) => c.url === '/api/meross/p1/dnd').map((c) => [c.init?.method, sentBody(c)])).toEqual([
			['POST', { enabled: true }],
			['POST', { enabled: false }]
		]);
	});

	it('an offline plug says so', async () => {
		plugServer(merossStatus({ online: false, wifi: { signal: null } }));
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByText(m.meross_not_connected())).toBeVisible();
		await expect.element(page.getByText(m.state_unreachable())).toBeVisible();
		await expect.element(page.getByText(m.meross_wifi_signal())).not.toBeInTheDocument();
	});

	it('a plug the server does not know: « not found »', async () => {
		stubFetch(() => json({ success: false, error: 'unknown plug' }, 404));
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByText(m.meross_not_found())).toBeVisible();
		await expect.element(page.getByRole('heading', { level: 1, name: m.meross_plug_control() })).toBeVisible();
	});

	it('says it is loading first', async () => {
		stubFetch(() => new Promise<Response>(() => {}));
		await render(PlugDetail, { id: 'p1' });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
	});
});
