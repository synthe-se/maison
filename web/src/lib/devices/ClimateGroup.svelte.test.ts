import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { BroadlinkClimateState, BroadlinkDevice } from '#lib/devices/climate/api.ts';
import { ui } from '#lib/ui.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { json, sentBody, stubFetch, type FetchCall } from '#lib/test/fetch.ts';
import { degrees } from './climate/labels.ts';
import ClimateGroup from './ClimateGroup.svelte';

beforeEach(() => session.adopt(leonard));
afterEach(() => {
	vi.useRealTimers();
});

const RM4: BroadlinkDevice = {
	host: '192.168.1.73',
	mac: 'aa',
	modelCode: 1,
	friendlyModel: 'RM4 Pro',
	friendlyType: 'RM4',
	name: 'RM4',
	isLocked: false,
	kind: 'rm',
	supportsLearning: true
};

const stored = (over: Partial<BroadlinkClimateState> = {}): BroadlinkClimateState => ({
	power: true,
	lastCommand: 'state-heat-23-fan-2-vane-low-stopin-90',
	lastOnCommand: 'state-heat-23-fan-2-vane-low-stopin-90',
	settings: { mode: 'heat', temperature: 23, fan: '2', vane: 'low', econo: false, stopInMinutes: 90 },
	host: RM4.host,
	model: 'msz-hj5va',
	updatedAt: new Date().toISOString(),
	...over
});

/** The Broadlink API: discovery, the stored state, and sends. */
function server({ devices = [RM4], state = null as BroadlinkClimateState | null } = {}) {
	return stubFetch((url) => {
		if (url.startsWith('/api/broadlink/discover')) return json({ success: true, devices, total: devices.length });
		if (url === '/api/broadlink/mitsubishi/state') return json({ success: true, state });
		return json({ success: true, result: { host: RM4.host, packetLength: 1 } });
	});
}
const sends = (calls: FetchCall[]) =>
	calls.filter((c) => c.url === '/api/broadlink/mitsubishi/send').map((c) => [c.init?.method, sentBody(c)]);
/** A list's trigger, named « label value » (Select.svelte). */
const choice = (label: string) => page.getByRole('button', { name: new RegExp(`^${label} `) });
const settings = () => page.getByRole('button', { name: m.common_settings_of({ name: m.climate_brand_name() }) });
const disclosure = () => page.getByRole('button', { name: m.climate_generated_command() });
/** The generated infrared order, folded under its disclosure: unfolded to be read. */
async function code() {
	if (disclosure().element().getAttribute('aria-expanded') !== 'true') await disclosure().click();
	return page.getByRole('code');
}
/** A choice of a single-choice group (ToggleGroup: a radio). */
const radio = (name: string) => page.getByRole('radio', { name });

describe('ClimateGroup', () => {
	it('no order yet: says so; Allumer sends the default settings to the RM4 Pro for the living-room model', async () => {
		const calls = server();
		const say = vi.spyOn(ui, 'say');
		await render(ClimateGroup);
		await expect.element(page.getByText(m.climate_no_order())).toBeVisible();
		const on = page.getByRole('button', { name: m.action_turn_on() });
		await expect.element(on).toBeEnabled();
		await on.click();
		await expect
			.poll(() => sends(calls))
			.toEqual([['POST', { host: RM4.host, command: 'state-cool-20-fan-auto-vane-auto-wide-center', model: 'msz-hj5va' }]]);
		await expect.poll(() => say.mock.calls.at(-1)?.[0]).toBe(m.climate_command_sent());
		// the state is read again after the order
		await expect.poll(() => calls.filter((c) => c.url === '/api/broadlink/mitsubishi/state').length).toBe(2);
	});

	it('Éteindre sends « state-off »', async () => {
		const calls = server();
		await render(ClimateGroup);
		await page.getByRole('button', { name: m.action_turn_off() }).click();
		await expect.poll(() => sends(calls).map(([, b]) => b.command)).toEqual(['state-off']);
	});

	it('the last order on: when, and the setpoint and mode as the fact', async () => {
		const s = stored();
		server({ state: s });
		await render(ClimateGroup);
		await expect.element(page.getByText(m.climate_fact({ temperature: degrees(23), mode: m.climate_modes_heat() }))).toBeVisible();
		await expect.element(page.getByText(m.climate_last_on({ when: '' }).trim(), { exact: false })).toBeVisible();
	});

	it('the last order off: no fact', async () => {
		server({ state: stored({ power: false }) });
		await render(ClimateGroup);
		await expect.element(page.getByText(m.climate_last_off({ when: '' }).trim(), { exact: false })).toBeVisible();
		await expect.element(page.getByText(degrees(23), { exact: false })).not.toBeInTheDocument();
	});

	it('the settings fill from the stored order once, and the generated command follows the form', async () => {
		const calls = server({ state: stored() });
		await render(ClimateGroup);
		await expect.element(settings()).toHaveAttribute('aria-expanded', 'false');
		await settings().click();
		await expect.element(settings()).toHaveAttribute('aria-expanded', 'true');
		await expect.element(page.getByText(m.climate_remote_connected({ host: RM4.host }))).toBeVisible();
		await expect.element(await code()).toHaveTextContent('state-heat-23-fan-2-vane-low-wide-center-stopin-90');
		// the restored timer is on, its odd duration kept in the list
		await expect.element(radio(m.climate_timer_modes_stop())).toHaveAttribute('aria-checked', 'true');
		await expect.element(radio(m.climate_timer_modes_none())).toHaveAttribute('aria-checked', 'false');
		// econo only means something in cool: it stays, not operable, saying why
		const econo = page.getByRole('switch', { name: m.climate_econo_cool() });
		await expect.element(econo).toHaveAttribute('aria-disabled', 'true');
		await expect.element(econo).toHaveAccessibleDescription(m.climate_econo_cool_description());
		await expect
			.element(page.getByRole('slider', { name: m.climate_temperature() }))
			.toHaveAttribute('aria-valuetext', m.climate_degrees_words({ degrees: 23 }));

		await radio(m.climate_timer_modes_none()).click();
		await expect.element(await code()).toHaveTextContent('state-heat-23-fan-2-vane-low-wide-center');
		await page.getByRole('button', { name: m.climate_send_structured_command() }).click();
		await expect.poll(() => sends(calls).map(([, b]) => b.command)).toEqual(['state-heat-23-fan-2-vane-low-wide-center']);
		// a re-read of the state does not overwrite the form
		await expect.element(radio(m.climate_timer_modes_none())).toHaveAttribute('aria-checked', 'true');
	});

	it('the temperature moves with the keyboard; Réinitialiser goes back to the defaults', async () => {
		server();
		await render(ClimateGroup);
		await settings().click();
		const slider = page.getByRole('slider', { name: m.climate_temperature() });
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await expect.element(await code()).toHaveTextContent('state-cool-30-fan-auto-vane-auto-wide-center');
		await page.getByRole('button', { name: m.climate_reset() }).click();
		await expect.element(await code()).toHaveTextContent('state-cool-20-fan-auto-vane-auto-wide-center');
	});

	it('econo and the sleep timer in cool', async () => {
		server();
		await render(ClimateGroup);
		await settings().click();
		const econo = page.getByRole('switch', { name: m.climate_econo_cool() });
		await expect.element(econo).not.toHaveAttribute('aria-disabled');
		await econo.click();
		await radio(m.climate_timer_modes_stop()).click();
		await expect.element(await code()).toHaveTextContent('state-cool-20-fan-auto-vane-auto-wide-center-econo-on-stopin-180');
	});

	it('choosing heat in the mode list turns econo off', async () => {
		server();
		await render(ClimateGroup);
		await settings().click();
		await page.getByRole('switch', { name: m.climate_econo_cool() }).click();
		await choice(m.climate_mode()).click();
		await page.getByRole('option', { name: m.climate_modes_heat() }).click();
		await expect.element(await code()).toHaveTextContent('state-heat-20-fan-auto-vane-auto-wide-center');
		await expect.element(page.getByRole('switch', { name: m.climate_econo_cool() })).toHaveAttribute('aria-disabled', 'true');
	});

	it('the fan, vane and stop-after lists write their tokens', async () => {
		server();
		await render(ClimateGroup);
		await settings().click();
		await choice(m.climate_fan()).click();
		await page.getByRole('option', { name: m.climate_fan_levels_silent() }).click();
		await choice(m.climate_vertical_vane()).click();
		await page.getByRole('option', { name: m.climate_vanes_swing() }).click();
		await radio(m.climate_timer_modes_stop()).click();
		await choice(m.climate_stop_after()).last().click();
		await page.getByRole('option').first().click();
		await expect.element(await code()).toHaveTextContent('state-cool-20-fan-silent-vane-swing-wide-center-stopin-30');
	});

	it('no remote yet: searching, the buttons saying why; after 2 min it says none answered and stops asking', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const calls = server({ devices: [] });
		await render(ClimateGroup);
		await vi.advanceTimersByTimeAsync(0);
		await expect.element(page.getByText(m.climate_searching_title(), { exact: false })).toBeVisible();
		const on = page.getByRole('button', { name: m.action_turn_on() });
		await expect.element(on).toHaveAttribute('aria-disabled', 'true');
		await expect.element(on).toHaveAccessibleDescription(new RegExp(m.climate_searching_title()));
		const discoveries = () => calls.filter((c) => c.url.startsWith('/api/broadlink/discover')).length;
		await expect.poll(discoveries).toBe(1);
		// every 4 s (each answer is awaited: a response body settles outside the fake clock)
		for (const n of [2, 3]) {
			await vi.advanceTimersByTimeAsync(4000);
			await expect.poll(discoveries).toBe(n);
		}
		await vi.advanceTimersByTimeAsync(120_000);
		await expect.element(page.getByText(m.climate_no_remote_title())).toBeVisible();
		const after = discoveries();
		await vi.advanceTimersByTimeAsync(60_000);
		expect(discoveries()).toBe(after);
	});

	it('the search button asks again, forcing a fresh discovery', async () => {
		const calls = server({ devices: [] });
		await render(ClimateGroup);
		const search = page.getByRole('button', { name: m.climate_search_remote() });
		await search.click();
		await expect.poll(() => calls.some((c) => c.url === '/api/broadlink/discover?forceRefresh=true')).toBe(true);
		await expect.element(search).not.toHaveAttribute('aria-busy');
		// once: the next reads go to the server's cache again
		await search.click();
		await expect.poll(() => calls.filter((c) => c.url === '/api/broadlink/discover?forceRefresh=true').length).toBe(2);
	});

	it('the order itself is folded away, under a disclosure', async () => {
		server({ state: stored() });
		await render(ClimateGroup);
		await settings().click();
		await expect.element(disclosure()).toHaveAttribute('aria-expanded', 'false');
		await expect.element(page.getByRole('code')).not.toBeInTheDocument();
		await disclosure().click();
		await expect.element(page.getByRole('code')).toBeVisible();
	});

	it('a member cannot force a network search (an admin’s)', async () => {
		session.adopt(alex);
		server({ devices: [] });
		await render(ClimateGroup);
		await expect.element(page.getByRole('heading', { name: m.climate_dashboard_title() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.climate_search_remote() })).not.toBeInTheDocument();
	});

	it('a failed send is told', async () => {
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		stubFetch((url) => {
			if (url.startsWith('/api/broadlink/discover')) return json({ success: true, devices: [RM4] });
			if (url === '/api/broadlink/mitsubishi/state') return json({ success: true, state: null });
			return json({ success: false, error: 'IR failed' }, 502);
		});
		await render(ClimateGroup);
		await page.getByRole('button', { name: m.action_turn_on() }).click();
		await expect.poll(() => fail.mock.calls.length).toBe(1);
		expect(String(fail.mock.calls[0][0])).toContain('IR failed');
	});
});
