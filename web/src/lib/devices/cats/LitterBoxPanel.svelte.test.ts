import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { formatDuration } from '#lib/format.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { fact, facts } from '#lib/test/facts.ts';
import LitterBoxPanel from './LitterBoxPanel.svelte';

const STATUS = '/devices/l1/litter-box/status';
const SETTINGS = '/devices/l1/litter-box/settings';
const box = (parsedStatus: unknown, more: Record<string, unknown> = {}) =>
	stubApi({ [STATUS]: { success: true, parsedStatus }, [`POST ${SETTINGS}`]: { success: true }, ...more });
const settingsSent = (api: ReturnType<typeof stubApi>) => api.sent('POST', SETTINGS).map((c) => c.body);

describe('LitterBoxPanel', () => {
	it('says it is loading until the box answers', async () => {
		stubApi({ [STATUS]: () => new Promise(() => {}) });
		await render(LitterBoxPanel, { id: 'l1' });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
	});

	it('says its state and levels in words, warning on what needs a hand', async () => {
		box({
			sensors: { litterLevel: 'half', faultAlarm: 4 },
			system: { state: 'satnd_by', maintenanceRequired: true },
			cleanDelay: { seconds: 300 }
		});
		await render(LitterBoxPanel, { id: 'l1' });
		await expect.element(page.getByText(m.litter_box_fill_soon())).toBeVisible();
		expect(facts()).toEqual([
			{ term: m.litter_box_litter_level(), value: m.litter_box_half_filled(), warn: true },
			{ term: m.common_status(), value: m.state_standby(), warn: false },
			{ term: m.common_status(), value: m.litter_box_maintenance_required(), warn: true },
			{ term: m.common_error(), value: m.litter_box_fault_alarm({ code: 4 }), warn: true }
		]);
		await expect
			.element(page.getByRole('slider', { name: m.litter_box_clean_delay_before() }))
			.toHaveAttribute('aria-valuetext', formatDuration(300));
	});

	it('shows a state it does not name as the device wrote it, and « Inconnu » without one', async () => {
		box({ sensors: { litterLevel: 'full' }, system: { state: 'dumping' } });
		await render(LitterBoxPanel, { id: 'l1' });
		await expect.element(page.getByText('dumping')).toBeVisible();
		expect(fact(m.litter_box_litter_level())).toEqual({ term: m.litter_box_litter_level(), value: m.litter_box_filled(), warn: false });
	});

	it('says « unknown » when the box says nothing, and starts the delay at 2 min', async () => {
		box({});
		await render(LitterBoxPanel, { id: 'l1' });
		await expect.element(page.getByRole('slider')).toHaveAttribute('aria-valuetext', formatDuration(120));
		expect(facts()).toEqual([{ term: m.common_status(), value: m.common_unknown(), warn: false }]);
	});

	it('cleans now, busy while the box starts, then says so', async () => {
		const answer = deferred();
		const api = box({}, { 'POST /devices/l1/litter-box/clean': () => answer.promise });
		const say = vi.spyOn(ui, 'say');
		await render(LitterBoxPanel, { id: 'l1' });
		await page.getByRole('button', { name: m.litter_box_start_cleaning() }).click();
		await expect.element(page.getByRole('button', { name: m.litter_box_cleaning() })).toBeDisabled();
		answer.resolve({ success: true });
		await expect.element(page.getByRole('button', { name: m.litter_box_start_cleaning() })).toBeEnabled();
		expect(api.sent('POST', '/devices/l1/litter-box/clean')).toHaveLength(1);
		expect(say).toHaveBeenCalledWith(m.litter_box_cleaning_started());
	});

	it('sets the delay from the keyboard, a minute a step', async () => {
		const api = box({ cleanDelay: { seconds: 120 } });
		await render(LitterBoxPanel, { id: 'l1' });
		const slider = page.getByRole('slider');
		await expect.element(slider).toHaveAttribute('aria-valuetext', formatDuration(120));
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		await expect.poll(() => settingsSent(api)).toEqual([{ cleanDelay: 180 }]);
	});

	it('marks the litter as full once confirmed', async () => {
		const api = box({ sensors: { litterLevel: 'half' } });
		const say = vi.spyOn(ui, 'say');
		await render(LitterBoxPanel, { id: 'l1' });
		await page.getByRole('button', { name: m.litter_box_reset_litter_level() }).click();
		await page
			.getByRole('alertdialog', { name: m.litter_box_reset_litter_level_title() })
			.getByRole('button', { name: m.litter_box_reset_action() })
			.click();
		await expect.poll(() => settingsSent(api)).toEqual([{ actions: { resetSandLevel: true } }]);
		await expect.poll(() => say).toHaveBeenCalledWith(m.device_setting_saved());
	});

	it('switches night mode and the preferences, each sent alone', async () => {
		const api = box({ sleepMode: { enabled: false }, settings: { childLock: true } });
		await render(LitterBoxPanel, { id: 'l1' });
		await page.getByRole('tab', { name: m.common_settings() }).click();
		await expect.element(page.getByRole('switch', { name: m.litter_box_child_lock() })).toBeChecked();
		await page.getByRole('switch', { name: m.litter_box_night_mode() }).click();
		await expect.poll(() => settingsSent(api)).toEqual([{ sleepMode: { enabled: true } }]);
		await page.getByRole('switch', { name: m.litter_box_child_lock() }).click();
		await page.getByRole('switch', { name: m.litter_box_kitten_mode() }).click();
		await expect.poll(() => settingsSent(api)).toHaveLength(3);
		expect(settingsSent(api).slice(1)).toEqual([{ preferences: { childLock: false } }, { preferences: { kittenMode: true } }]);
	});

	it('applies night hours typed, starting from the box’s, else 23:00 to 07:00', async () => {
		const api = box({ sleepMode: { enabled: true, startTimeFormatted: '22:00' } });
		await render(LitterBoxPanel, { id: 'l1' });
		await page.getByRole('tab', { name: m.common_settings() }).click();
		const start = page.getByLabelText(m.litter_box_start(), { exact: true });
		const end = page.getByLabelText(m.litter_box_end(), { exact: true });
		await expect.element(start).toHaveValue('22:00');
		await expect.element(end).toHaveValue('07:00');
		await end.fill('06:30');
		await page.getByRole('button', { name: m.litter_box_apply_schedule() }).click();
		await expect.poll(() => settingsSent(api)).toEqual([{ sleepMode: { startTime: '22:00', endTime: '06:30' } }]);
		await expect.element(page.getByRole('group', { name: m.litter_box_night_hours() })).toBeVisible();
	});

	it('offline: « Lancer le nettoyage » stays, unavailable, saying why; no delay nor « J’ai rempli »; « Reconnecter » connects', async () => {
		const api = box({ system: { state: 'satnd_by' } }, { 'POST /devices/l1/connect': { success: true } });
		await render(LitterBoxPanel, { id: 'l1', device: { id: 'l1', name: 'Litière', connected: false } });
		const clean = page.getByRole('button', { name: m.litter_box_start_cleaning() });
		await expect.element(clean).toHaveAttribute('aria-disabled', 'true');
		await expect.element(clean).toHaveAccessibleDescription(m.litter_box_offline_reason());
		(clean.element() as HTMLElement).click();
		expect(api.sent('POST', '/devices/l1/litter-box/clean')).toHaveLength(0);
		await expect.element(page.getByRole('slider')).not.toBeInTheDocument();
		await expect.element(page.getByRole('button', { name: m.litter_box_reset_litter_level() })).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.device_reconnect() }).click();
		await expect.poll(() => api.sent('POST', '/devices/l1/connect')).toHaveLength(1);
	});
});
