import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { formatDuration } from '#lib/format.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi } from '#lib/test/api.ts';
import { fact, facts } from '#lib/test/facts.ts';
import LitterBoxControl from './LitterBoxControl.svelte';

const STATUS = '/devices/l1/litter-box/status';
const SETTINGS = '/devices/l1/litter-box/settings';
const box = (parsed_status: unknown, more: Record<string, unknown> = {}) =>
	stubApi({ [STATUS]: { success: true, parsed_status }, [`POST ${SETTINGS}`]: { success: true }, ...more });
const settingsSent = (api: ReturnType<typeof stubApi>) => api.sent('POST', SETTINGS).map((c) => c.body);

describe('LitterBoxControl', () => {
	afterEach(() => forgetAll());

	it('says it is loading until the box answers', async () => {
		stubApi({ [STATUS]: () => new Promise(() => {}) });
		await render(LitterBoxControl, { id: 'l1' });
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
	});

	it('says its state and levels in words, warning on what needs a hand', async () => {
		box({ sensors: { litter_level: 'half', fault_alarm: 4 }, system: { state: 'satnd_by', maintenance_required: true }, clean_delay: { seconds: 300 } });
		await render(LitterBoxControl, { id: 'l1' });
		await expect.element(page.getByText(m.litter_box_fill_soon())).toBeVisible();
		expect(facts()).toEqual([
			{ term: m.litter_box_litter_level(), value: m.litter_box_half_filled(), warn: true },
			{ term: m.common_status(), value: m.state_standby(), warn: false },
			{ term: m.common_status(), value: m.litter_box_maintenance_required(), warn: true },
			{ term: m.common_error(), value: m.litter_box_fault_alarm({ code: 4 }), warn: true }
		]);
		await expect.element(page.getByRole('slider', { name: m.litter_box_clean_delay_before() })).toHaveAttribute('aria-valuetext', formatDuration(300));
	});

	it('shows a state it does not name as the device wrote it, and « Inconnu » without one', async () => {
		box({ sensors: { litter_level: 'full' }, system: { state: 'dumping' } });
		await render(LitterBoxControl, { id: 'l1' });
		await expect.element(page.getByText('dumping')).toBeVisible();
		expect(fact(m.litter_box_litter_level())).toEqual({ term: m.litter_box_litter_level(), value: m.litter_box_filled(), warn: false });
	});

	it('says « unknown » when the box says nothing, and starts the delay at 2 min', async () => {
		box({});
		await render(LitterBoxControl, { id: 'l1' });
		await expect.element(page.getByRole('slider')).toHaveAttribute('aria-valuetext', formatDuration(120));
		expect(facts()).toEqual([{ term: m.common_status(), value: m.common_unknown(), warn: false }]);
	});

	it('cleans now, busy while the box starts, then says so', async () => {
		const answer = deferred();
		const api = box({}, { 'POST /devices/l1/litter-box/clean': () => answer.promise });
		const say = vi.spyOn(ui, 'say');
		await render(LitterBoxControl, { id: 'l1' });
		await page.getByRole('button', { name: m.litter_box_start_cleaning() }).click();
		await expect.element(page.getByRole('button', { name: m.litter_box_cleaning() })).toBeDisabled();
		answer.resolve({ success: true });
		await expect.element(page.getByRole('button', { name: m.litter_box_start_cleaning() })).toBeEnabled();
		expect(api.sent('POST', '/devices/l1/litter-box/clean')).toHaveLength(1);
		expect(say).toHaveBeenCalledWith(m.litter_box_cleaning_started());
	});

	it('sets the delay from the keyboard, a minute a step', async () => {
		const api = box({ clean_delay: { seconds: 120 } });
		await render(LitterBoxControl, { id: 'l1' });
		const slider = page.getByRole('slider');
		await expect.element(slider).toHaveAttribute('aria-valuetext', formatDuration(120));
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		await expect.poll(() => settingsSent(api)).toEqual([{ clean_delay: 180 }]);
	});

	it('marks the litter as full once confirmed', async () => {
		const api = box({ sensors: { litter_level: 'half' } });
		const say = vi.spyOn(ui, 'say');
		await render(LitterBoxControl, { id: 'l1' });
		await page.getByRole('button', { name: m.litter_box_reset_litter_level() }).click();
		await page.getByRole('alertdialog', { name: m.litter_box_reset_litter_level_title() }).getByRole('button', { name: m.litter_box_reset_action() }).click();
		await expect.poll(() => settingsSent(api)).toEqual([{ actions: { reset_sand_level: true } }]);
		await expect.poll(() => say).toHaveBeenCalledWith(m.device_setting_saved());
	});

	it('switches night mode and the preferences, each sent alone', async () => {
		const api = box({ sleep_mode: { enabled: false }, settings: { child_lock: true } });
		await render(LitterBoxControl, { id: 'l1' });
		await page.getByRole('tab', { name: m.litter_box_settings() }).click();
		await expect.element(page.getByRole('switch', { name: m.litter_box_child_lock() })).toBeChecked();
		await page.getByRole('switch', { name: m.litter_box_night_mode() }).click();
		await expect.poll(() => settingsSent(api)).toEqual([{ sleep_mode: { enabled: true } }]);
		await page.getByRole('switch', { name: m.litter_box_child_lock() }).click();
		await page.getByRole('switch', { name: m.litter_box_kitten_mode() }).click();
		await expect.poll(() => settingsSent(api)).toHaveLength(3);
		expect(settingsSent(api).slice(1)).toEqual([{ preferences: { child_lock: false } }, { preferences: { kitten_mode: true } }]);
	});

	it('applies night hours typed, starting from the box’s, else 23:00 to 07:00', async () => {
		const api = box({ sleep_mode: { enabled: true, start_time_formatted: '22:00' } });
		await render(LitterBoxControl, { id: 'l1' });
		await page.getByRole('tab', { name: m.litter_box_settings() }).click();
		const start = page.getByLabelText(m.litter_box_start(), { exact: true });
		const end = page.getByLabelText(m.litter_box_end(), { exact: true });
		await expect.element(start).toHaveValue('22:00');
		await expect.element(end).toHaveValue('07:00');
		await end.fill('06:30');
		await page.getByRole('button', { name: m.litter_box_apply_schedule() }).click();
		await expect.poll(() => settingsSent(api)).toEqual([{ sleep_mode: { start_time: '22:00', end_time: '06:30' } }]);
		await expect.element(page.getByRole('group', { name: m.litter_box_night_hours() })).toBeVisible();
	});
});
