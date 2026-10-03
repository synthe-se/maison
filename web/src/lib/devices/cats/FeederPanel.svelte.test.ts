import { describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { when } from '#lib/i18n.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { fact, facts } from '#lib/test/facts.ts';
import FeederPanel from './FeederPanel.svelte';

const STATUS = '/devices/f1/feeder/status';
const MEALS = '/devices/f1/feeder/meal-plan';
const withStatus = (parsedStatus: unknown, more: Record<string, unknown> = {}) =>
	stubApi({ [STATUS]: { success: true, parsedStatus }, [MEALS]: { success: true, decoded: [] }, ...more });

describe('FeederPanel', () => {
	it('says what the feeder reports, in words, and no fault it cannot read', async () => {
		withStatus({
			feeding: { manualFeedEnabled: true, lastFeedSize: '2 portions', lastFeedReport: 0, quickFeedAvailable: false },
			system: { poweredBy: 'Battery', ipAddress: '10.0.0.2' },
			history: { raw: 'R:0 C:2 T:1773270006', parsed: { remaining: '0', count: '2', timestamp: '1773270006', timestampReadable: '' } }
		});
		await render(FeederPanel, { id: 'f1' });
		await expect.element(page.getByText(m.feeder_power_battery())).toBeVisible();
		expect(facts()).toEqual([
			{ term: m.feeder_power_source(), value: m.feeder_power_battery(), warn: false },
			{
				term: m.feeder_last_meal(),
				value: m.feeder_last_meal_value({ portions: m.feeder_portion({ count: 2 }), when: when(1773270006000) }),
				warn: false
			}
		]);
	});

	it('says only what it knows, and the backend’s own word for a power it does not name', async () => {
		withStatus({ system: { poweredBy: 'Mode 7' }, history: null });
		await render(FeederPanel, { id: 'f1' });
		await expect.element(page.getByText('Mode 7')).toBeVisible();
		expect(facts()).toEqual([
			{ term: m.feeder_power_source(), value: 'Mode 7', warn: false },
			{ term: m.feeder_last_meal(), value: m.common_unknown(), warn: false }
		]);
		expect(fact(m.common_status())).toBeUndefined();
	});

	it('says when the status cannot be read', async () => {
		stubApi({ [STATUS]: new Response('{"error":"offline"}', { status: 503 }), [MEALS]: { success: true, decoded: [] } });
		await render(FeederPanel, { id: 'f1' });
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
	});

	it('serves the portions chosen, the count in the button, and says when it served', async () => {
		const answer = deferred();
		const api = withStatus({ system: { poweredBy: 'AC Power' } }, { 'POST /devices/f1/feeder/feed': () => answer.promise });
		await render(FeederPanel, { id: 'f1' });
		const slider = page.getByRole('slider', { name: m.feeder_portions() });
		await slider.click();
		await userEvent.keyboard('{Home}{ArrowRight}{ArrowRight}');
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.feeder_portion({ count: 3 }));
		await page.getByRole('button', { name: m.feeder_distribute({ count: 3 }) }).click();
		const busy = page.getByRole('button', { name: m.feeder_distributing() });
		await expect.element(busy).toBeDisabled();
		await expect.element(busy).toHaveAttribute('aria-busy', 'true');
		answer.resolve({ success: true });
		await expect.element(page.getByText(m.feeder_served_at({ time: '' }).trim(), { exact: false })).toBeVisible();
		expect(api.sent('POST', '/devices/f1/feeder/feed')[0].body).toEqual({ portion: 3 });
	});

	it('keeps the scheduled meals in the second tab', async () => {
		withStatus({});
		await render(FeederPanel, { id: 'f1' });
		await page.getByRole('tab', { name: m.feeder_schedule() }).click();
		await expect.element(page.getByText(m.feeder_meal_schedule_description())).toBeVisible();
		await expect.element(page.getByText(m.meal_plan_no_meals())).toBeVisible();
	});

	it('offline: the serve button stays, unavailable, saying why, with « Reconnecter »', async () => {
		const api = stubApi({ '/devices/f1/feeder/status': { success: true, parsedStatus: {} } });
		await render(FeederPanel, { id: 'f1', device: { id: 'f1', name: 'Distributeur', connected: false } });
		const serve = page.getByRole('button', { name: m.feeder_distribute({ count: 1 }) });
		await expect.element(serve).toHaveAttribute('aria-disabled', 'true');
		await expect.element(serve).toHaveAccessibleDescription(m.cats_offline_reason());
		(serve.element() as HTMLElement).click();
		expect(api.sent('POST', '/devices/f1/feeder/feed')).toHaveLength(0);
		await expect.element(page.getByRole('button', { name: m.device_reconnect() })).toBeVisible();
	});
});
