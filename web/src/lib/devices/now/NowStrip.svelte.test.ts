import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { clock, inSentence, num } from '#lib/i18n.svelte.ts';
import { time } from '#lib/clock.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import { TARIFFS, tempoToday } from '#lib/test/tempo.ts';
import { hueLamp, zigbeeLamp } from '#lib/test/lamps.ts';
import { merossElectricity, merossPlug } from '#lib/test/meross.ts';
import { shutter } from '#lib/test/shutters.ts';
import { tuyaDevice } from '#lib/test/tuya.ts';
import NowStrip from './NowStrip.svelte';

// a laptop's width: every chip of the house fits on two lines (the folding is tested apart)
beforeEach(() => page.viewport(1280, 800));
afterEach(async () => {
	time.now = new Date();
	await page.viewport(414, 896);
});

const close = new Date('2026-12-10T18:42:00');
/** A red peak afternoon: two lamps lit, a shutter closing at 18:42, the fountain low on
 * water, a plug drawing 1 kW, a lamp and a plug out of reach. */
function house() {
	time.now = new Date('2026-12-10T15:00:00');
	const big = merossElectricity('p1');
	big.electricity.raw.power = 1_040_000;
	return stubApi({
		'/tempo': tempoToday(),
		'/hue-lamps': { success: true, lamps: [hueLamp(), hueLamp({ id: 'h2', connected: false })] },
		'/zigbee/lamps': { success: true, lamps: [zigbeeLamp()] },
		'/meross': {
			success: true,
			devices: [merossPlug({ name: 'Lave-linge' }), merossPlug({ id: 'p2', isOnline: false })],
			total: 2,
			message: ''
		},
		'/meross/p1/electricity': big,
		'/devices': { success: true, devices: [tuyaDevice({ id: 'w1', name: 'Fontaine', type: 'fountain' })], total: 1, message: '' },
		'/devices/w1/fountain/status': { success: true, parsedStatus: { waterLevel: 'low' } },
		'/matter/covers': { success: true, covers: [shutter({ name: 'Volet salon', nextClose: close.toISOString() })] }
	});
}
const chip = (name: string) => page.getByRole('region', { name: m.now_title() }).getByRole('button', { name });

describe('NowStrip', () => {
	it('sums up the house in chips, what needs a hand in the warning style', async () => {
		house();
		await render(NowStrip);
		const tempo = chip(
			m.now_tempo({
				color: m.color_red(),
				period: m.tempo_hp(),
				cents: num(TARIFFS.red.peak * 100, 1),
				time: clock(new Date(2000, 0, 1, 22))
			})
		);
		await expect.element(tempo).toBeVisible();
		await expect.element(tempo).toHaveClass('warn'); // a red peak
		await expect.element(chip(m.now_lamps_on({ count: 2 }))).not.toHaveClass('warn');
		await expect.element(chip(m.shutters_will_close({ name: 'Volet salon', time: clock(close) }))).toBeVisible();
		await expect.element(chip(m.now_alert({ name: 'Fontaine', what: inSentence(m.cats_water_low()) }))).toHaveClass('warn');
		await expect.element(chip(m.now_unreachable({ count: 2 }))).toHaveClass('warn');
		await expect.element(page.getByRole('button', { name: /Lave-linge/ })).toHaveClass('warn');
	});

	it('nothing to say: no chip at all', async () => {
		stubApi({});
		await render(NowStrip);
		await expect.element(page.getByRole('region', { name: m.now_title() })).toBeInTheDocument();
		expect(document.querySelectorAll('.now li:not([hidden])')).toHaveLength(0);
	});

	it('a chip leads to its group: its title scrolled to and focused', async () => {
		house();
		await render(NowStrip);
		const group = document.createElement('section');
		group.innerHTML = '<h2 id="lamps-title">Lampes</h2>';
		document.body.append(group);
		await chip(m.now_lamps_on({ count: 2 })).click();
		await expect.element(page.getByRole('heading', { name: 'Lampes' })).toHaveFocus();
		group.remove();
	});

	it('two lines at most: the rest waits behind « +N », what needs a hand kept in sight', async () => {
		house();
		await render(NowStrip);
		await expect.element(chip(m.now_lamps_on({ count: 2 }))).toBeVisible();
		(document.querySelector('.now') as HTMLElement).style.width = '260px';
		await expect
			.poll(
				() =>
					[...document.querySelectorAll<HTMLElement>('.now li')]
						.filter((li) => !li.hidden)
						.map((li) => li.offsetTop)
						.filter((v, i, a) => a.indexOf(v) === i).length
			)
			.toBeLessThanOrEqual(2);
		// « +N »: the chips folded behind it, named for readers
		const more = page.getByRole('button', { name: new RegExp(`^${m.now_more({ count: 9 }).replace('9', '\\d+')}$`) });
		await expect.element(more).toHaveAttribute('aria-expanded', 'false');
		// the warnings stay in sight before the facts
		expect(document.querySelectorAll('.now li:not([hidden]) .warn').length).toBeGreaterThan(0);
		expect(document.querySelector('.now li:not([hidden]) .now-chip:not(.warn):not([aria-expanded])')).toBeNull();
		await more.click();
		await expect.element(chip(m.now_lamps_on({ count: 2 }))).toBeVisible();
		expect(document.activeElement?.classList.contains('now-chip')).toBe(true);
	});
});
