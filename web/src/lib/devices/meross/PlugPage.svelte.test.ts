import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { json, sentBody, stubFetch } from '#lib/test/fetch.ts';
import { merossElectricity, merossStatus } from '#lib/test/meross.ts';
import { kwh, kwhFromWh, milliamps, powerAndCost, volts, watts } from './units.ts';
import { percent } from '#lib/i18n.svelte.ts';
import { time } from '#lib/clock.svelte.ts';
import { TARIFFS, tempoStock, tempoToday } from '#lib/test/tempo.ts';
import { euros, monthEstimate } from '#lib/devices/tempo/price.ts';
import PlugPage from './PlugPage.svelte';

const consumption = {
	success: true,
	device: { id: 'p1', name: 'Radiateur' },
	consumption: Array.from({ length: 9 }, (_, i) => ({ date: `2026-09-${String(20 + i).padStart(2, '0')}`, time: 0, value: 1000 + i })),
	summary: { days: 9, totalWh: 9036, totalKwh: 9.036 },
	message: ''
};

/** Answers each of the page's three reads; records every call. */
function plugServer(status = merossStatus(), tempo?: { today: unknown; calendar: unknown }) {
	return stubFetch((url) => {
		if (url.startsWith('/api/tempo')) {
			if (!tempo) return json({ success: false, error: 'no Tempo' }, 503);
			return json(url === '/api/tempo' ? tempo.today : tempo.calendar);
		}
		if (url === '/api/meross/p1/status') return json(status);
		if (url === '/api/meross/p1/electricity') return json(merossElectricity());
		if (url === '/api/meross/p1/consumption') return json(consumption);
		return json({ success: true });
	});
}

describe('PlugPage', () => {
	it('reads status, electricity and consumption once each', async () => {
		const calls = plugServer();
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByRole('heading', { level: 1, name: 'Radiateur' })).toBeVisible();
		expect(
			calls
				.map((c) => c.url)
				.filter((u) => u.startsWith('/api/meross'))
				.toSorted()
		).toEqual(['/api/meross/p1/consumption', '/api/meross/p1/electricity', '/api/meross/p1/status']);
		await expect.element(page.getByRole('button', { name: 'Radiateur' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByRole('link', { name: 'Radiateur' })).not.toBeInTheDocument();
	});

	it('lists the last seven days, newest first, and the period’s total', async () => {
		plugServer();
		await render(PlugPage, { id: 'p1' });
		const days = page.getByRole('region', { name: m.meross_consumption() }).getByRole('definition');
		await expect.poll(() => days.elements().length).toBe(7);
		expect(days.elements()[0].textContent).toBe(kwhFromWh(1008));
		expect(days.elements()[6].textContent).toBe(kwhFromWh(1002));
		await expect.element(page.getByText(`${m.meross_consumption_days({ count: 9 })} · ${kwh(9.036)}`)).toBeVisible();
	});

	it('names the hardware, unknown values in words', async () => {
		plugServer(merossStatus({ hardware: { type: 'mss310', version: '', chipType: 'mt7682', uuid: 'u', mac: 'aa' } }));
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByText('MSS310')).toBeVisible();
		await expect.element(page.getByText(m.common_unknown())).toBeVisible();
		await expect.element(page.getByText(m.meross_wifi_signal())).toBeVisible();
	});

	// the backend sends « 230.4V », « 0.183A », « 42.4W » (meross.rs format_*): read as numbers
	// they are NaN. The tile already reads `raw` (livePower); the page does not.
	it('says voltage, current and power in numbers', async () => {
		plugServer();
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByText(volts(230.4)), { timeout: 1000 }).toBeVisible();
		await expect.element(page.getByText(milliamps(0.183)), { timeout: 1000 }).toBeVisible();
		await expect.element(page.getByText(watts(42.4, 1)).first(), { timeout: 1000 }).toBeVisible();
	});

	it('the indicator light: two orders, each a POST to dnd', async () => {
		const calls = plugServer();
		const toast = vi.spyOn(ui, 'toast');
		await render(PlugPage, { id: 'p1' });
		await page.getByRole('button', { name: m.meross_led_off() }).click();
		await expect.poll(() => toast.mock.calls.length).toBe(1);
		expect(toast).toHaveBeenCalledWith(m.meross_dnd_updated());
		await page.getByRole('button', { name: m.meross_led_on() }).click();
		await expect
			.poll(() => calls.filter((c) => c.url === '/api/meross/p1/dnd').map((c) => [c.init?.method, sentBody(c)]))
			.toEqual([
				['POST', { enabled: true }],
				['POST', { enabled: false }]
			]);
	});

	it('an offline plug says so', async () => {
		plugServer(merossStatus({ online: false, wifi: { signal: null } }));
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByText(m.meross_not_connected())).toBeVisible();
		await expect.element(page.getByText(m.state_never_seen())).toBeVisible();
		await expect.element(page.getByText(m.meross_wifi_signal())).not.toBeInTheDocument();
	});

	it('a plug the server does not know (404): « not found », nothing to retry', async () => {
		stubFetch(() => json({ success: false, error: 'unknown plug', code: 'not_found' }, 404));
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByText(m.meross_not_found())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_retry() })).not.toBeInTheDocument();
		await expect.element(page.getByRole('heading', { level: 1, name: m.meross_plug_control() })).toBeVisible();
	});

	it('says it is loading first', async () => {
		stubFetch(() => new Promise<Response>(() => {}));
		await render(PlugPage, { id: 'p1' });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
	});

	it('says what the month cost so far, each day at its Tempo color, as a stated estimate', async () => {
		time.now = new Date('2026-09-28T23:00:00'); // a blue night: off-peak
		const calendar = {
			success: true,
			season: '2025-2026',
			calendar: consumption.consumption.map((d) => ({ date: d.date, color: 'BLUE', isActual: true, isPrediction: false })),
			stock: tempoStock()
		};
		plugServer(merossStatus(), {
			today: tempoToday({ yesterday: { date: '2026-09-27', color: 'BLUE' }, today: { date: '2026-09-28', color: 'BLUE' } }),
			calendar
		});
		await render(PlugPage, { id: 'p1' });
		const expected = monthEstimate(
			consumption.consumption.map((d) => ({ date: d.date, wh: d.value })),
			() => 'BLUE',
			TARIFFS,
			{ peakStart: '06:00', peakEnd: '22:00' },
			'2026-09'
		);
		await expect.element(page.getByText(m.meross_month_estimate({ cost: euros(expected.euros) }))).toBeVisible();
		await expect.element(page.getByText(m.meross_estimate(), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.meross_estimate_hint({ count: 9, peak: percent(2 / 3) }))).toBeVisible();
		// and the live power with what it costs now
		await expect.element(page.getByText(powerAndCost(42.4, TARIFFS.blue.offPeak, 1))).toBeVisible();
		time.now = new Date();
	});
});
