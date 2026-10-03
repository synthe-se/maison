import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { IrAction } from '#lib/devices/remote/api.ts';
import { degrees, MODE_LABEL, FAN_LABEL, VANE_LABEL } from '#lib/devices/climate/labels.ts';
import { remoteSources } from '#lib/test/remote.ts';
import ActionRow from './ActionRow.svelte';
import { ACTION_TYPES, NABAZTAG_PRESETS, SCENE_TYPES, SWITCH_STATES, summarize } from './actions.ts';

/** A Select's trigger, named « label value ». */
const choice = (label: string) => page.getByRole('button', { name: new RegExp(`^${label.replace(/[()]/g, '\\$&')}`) });
const pick = async (label: string, option: string) => {
	await choice(label).click();
	await page.getByRole('option', { name: option, exact: true }).click();
};

async function row(action: IrAction | { action: string }, index = 0, count = 1) {
	const onchange = vi.fn();
	const onmove = vi.fn();
	const onremove = vi.fn();
	await render(ActionRow, { action: action as IrAction, index, count, sources: remoteSources(), onchange, onmove, onremove });
	return { onchange, onmove, onremove };
}

describe('ActionRow', () => {
	it('is named by its place, and moves or goes from the keyboard-reachable buttons', async () => {
		const { onmove, onremove } = await row({ action: 'nabaztag', command: 'ping' }, 1, 3);
		await expect.element(page.getByRole('listitem', { name: m.remote_action_number({ n: 2 }) })).toBeVisible();
		await page.getByRole('button', { name: m.remote_move_up({ n: 2 }) }).click();
		expect(onmove).toHaveBeenCalledWith(-1);
		await page.getByRole('button', { name: m.remote_move_down({ n: 2 }) }).click();
		expect(onmove).toHaveBeenLastCalledWith(1);
		await page.getByRole('button', { name: m.remote_remove_action({ n: 2 }) }).click();
		expect(onremove).toHaveBeenCalledOnce();
	});

	it('cannot move the first up nor the last down', async () => {
		await row({ action: 'nabaztag', command: 'ping' }, 0, 1);
		await expect.element(page.getByRole('button', { name: m.remote_move_up({ n: 1 }) })).toBeDisabled();
		await expect.element(page.getByRole('button', { name: m.remote_move_down({ n: 1 }) })).toBeDisabled();
	});

	it('changing the type starts a new action of that type on the first device', async () => {
		const { onchange } = await row({ action: 'nabaztag', command: 'ping' });
		await expect.element(choice(m.remote_action_type())).toHaveTextContent(ACTION_TYPES.nabaztag());
		await pick(m.remote_action_type(), ACTION_TYPES.meross_power());
		expect(onchange).toHaveBeenCalledWith({ action: 'meross_power', device: 'p1', state: 'toggle' });
	});

	it('nabaztag: a command typed, or a suggestion', async () => {
		const { onchange } = await row({ action: 'nabaztag', command: '' });
		const input = page.getByRole('textbox', { name: m.remote_fields_command() });
		await expect.element(input).toHaveAccessibleDescription(m.remote_fields_command_hint());
		await input.fill('ears 3 5');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'nabaztag', command: 'ears 3 5' });
		await expect
			.element(page.getByRole('group', { name: m.remote_fields_presets() }).getByRole('button'))
			.toHaveLength(NABAZTAG_PRESETS.length);
		await page.getByRole('button', { name: 'dance 1' }).click();
		expect(onchange).toHaveBeenLastCalledWith({ action: 'nabaztag', command: 'dance 1' });
	});

	it('zigbee power: a lamp by its name, and a state', async () => {
		const { onchange } = await row({ action: 'zigbee_power', lamp: '', state: 'on' });
		await expect.element(choice(m.lamps_lamp())).toHaveTextContent(m.remote_fields_lamp_placeholder());
		await pick(m.lamps_lamp(), 'Suspension');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'zigbee_power', lamp: 'zb-1', state: 'on' });
		await pick(m.remote_fields_state(), SWITCH_STATES.off());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'zigbee_power', lamp: '', state: 'off' });
	});

	it('hue power: a Hue lamp by its name, and a state', async () => {
		const { onchange } = await row({ action: 'hue_power', lamp: '', state: 'toggle' });
		await pick(m.lamps_lamp(), 'Lampe du salon');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'hue_power', lamp: 'hue-1', state: 'toggle' });
		await pick(m.remote_fields_state(), SWITCH_STATES.off());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'hue_power', lamp: '', state: 'off' });
	});

	it('hue brightness: a percentage, 1 at the least', async () => {
		const { onchange } = await row({ action: 'hue_brightness', lamp: 'hue-1', brightness: 1 });
		const slider = page.getByRole('slider', { name: m.lamps_brightness() });
		await expect.element(slider).toHaveAttribute('aria-valuemin', '1');
		await expect.element(slider).toHaveAttribute('aria-valuemax', '100');
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowLeft}');
		await userEvent.keyboard('{ArrowRight}');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'hue_brightness', lamp: 'hue-1', brightness: 2 });
	});

	it('cover: a shutter, a move, and a position only for a position', async () => {
		const { onchange } = await row({ action: 'cover', cover: '', command: 'close' });
		await expect.element(page.getByRole('slider')).not.toBeInTheDocument();
		await pick(m.remote_fields_cover(), 'Salon');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'cover', cover: 's1', command: 'close' });
		await pick(m.remote_fields_command(), m.remote_fields_cover_position());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'cover', cover: '', command: 'position', position: 50 });
	});

	it('cover position: the open percentage in words; another move drops it', async () => {
		const { onchange } = await row({ action: 'cover', cover: 's1', command: 'position', position: 30 });
		const slider = page.getByRole('slider', { name: m.shutters_position() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.shutters_open_percent({ percent: 30 }));
		await pick(m.remote_fields_command(), m.shutters_open_action());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'cover', cover: 's1', command: 'open' });
	});

	it('climate on and off: the blaster; « on » says what it resumes', async () => {
		const { onchange } = await row({ action: 'climate_on', host: '' });
		await expect.element(page.getByText(m.remote_fields_climate_on_hint())).toBeVisible();
		await pick(m.remote_fields_host(), 'RM4 salon (192.0.2.60)');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'climate_on', host: '192.0.2.60' });
	});

	it('zigbee brightness: a lamp and a level said in words', async () => {
		const { onchange } = await row({ action: 'zigbee_brightness', lamp: '', brightness: 127 });
		const slider = page.getByRole('slider', { name: m.lamps_brightness() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.remote_brightness_value({ value: 127 }));
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'zigbee_brightness', lamp: '', brightness: 128 });
		await pick(m.lamps_lamp(), 'Suspension');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'zigbee_brightness', lamp: 'zb-1', brightness: 127 });
	});

	it('broadlink: a device and a saved code, by their names', async () => {
		const { onchange } = await row({ action: 'broadlink_code', host: '', codeId: '' });
		await pick(m.remote_fields_host(), 'RM4 salon (192.0.2.60)');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'broadlink_code', host: '192.0.2.60', codeId: '' });
		await pick(m.remote_fields_code(), 'Ventilateur');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'broadlink_code', host: '', codeId: 'fan-on' });
	});

	it('meross: a plug by its name and address, and a state', async () => {
		const { onchange } = await row({ action: 'meross_power', device: '', state: 'toggle' });
		await pick(m.remote_fields_device(), 'Radiateur (192.168.1.40)');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'meross_power', device: 'p1', state: 'toggle' });
		await pick(m.remote_fields_state(), SWITCH_STATES.on());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'meross_power', device: '', state: 'on' });
	});

	it('climate: the settings in pickers, rebuilt into the command', async () => {
		const { onchange } = await row({ action: 'climate_toggle', host: '192.0.2.60', onCommand: 'state-cool-16-fan-4-vane-swing' });
		await expect.element(page.getByText(m.remote_fields_climate_hint())).toBeVisible();
		await pick(m.climate_mode(), MODE_LABEL.heat());
		expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ onCommand: 'state-heat-16-fan-4-vane-swing' }));
		await pick(m.climate_fan(), FAN_LABEL.silent());
		expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ onCommand: 'state-cool-16-fan-silent-vane-swing' }));
		await pick(m.climate_vertical_vane(), VANE_LABEL.low());
		expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ onCommand: 'state-cool-16-fan-4-vane-low' }));
		const temperature = page.getByRole('slider', { name: m.climate_temperature() });
		await expect.element(temperature).toHaveAttribute('aria-valuetext', degrees(16));
		(temperature.element() as HTMLElement).focus();
		await userEvent.keyboard('{ArrowRight}');
		expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ onCommand: 'state-cool-17-fan-4-vane-swing' }));
		await pick(m.remote_fields_host(), 'RM4 salon (192.0.2.60)');
		expect(onchange).toHaveBeenLastCalledWith(expect.objectContaining({ host: '192.0.2.60' }));
	});

	it('climate: a command the pickers cannot hold stays editable as text', async () => {
		const { onchange } = await row({ action: 'climate_toggle', host: 'h', onCommand: 'state-cool-20-fan-1-vane-auto-stop-22-30' });
		const raw = page.getByRole('textbox', { name: m.remote_fields_climate_raw_command() });
		await expect.element(raw).toHaveValue('state-cool-20-fan-1-vane-auto-stop-22-30');
		await expect.element(choice(m.climate_mode())).not.toBeInTheDocument();
		await raw.fill('state-off');
		expect(onchange).toHaveBeenLastCalledWith({ action: 'climate_toggle', host: 'h', onCommand: 'state-off' });
	});

	it('an action it does not edit is shown by its summary, and kept', async () => {
		await row({ action: 'tv_key' });
		await expect.element(page.getByText(summarize({ action: 'tv_key' }, remoteSources()))).toBeVisible();
		await expect.element(page.getByText(m.remote_other_hint())).toBeVisible();
		await expect.element(choice(m.remote_action_type())).not.toBeInTheDocument();
	});

	it('a remote key can run a scene: the scene picked by name', async () => {
		const { onchange } = await row({ action: 'scene', scene: 'nuit' });
		await expect.element(choice(m.remote_fields_scene())).toHaveTextContent('Nuit');
		await pick(m.remote_action_type(), ACTION_TYPES.tv_power());
		expect(onchange).toHaveBeenLastCalledWith({ action: 'tv_power', state: 'on', switchToBox: true });
	});

	it('the TV on or off, an app on the box', async () => {
		const tv = await row({ action: 'tv_power', state: 'on' });
		await pick(m.remote_fields_state(), SWITCH_STATES.off());
		expect(tv.onchange).toHaveBeenLastCalledWith({ action: 'tv_power', state: 'off' });
	});

	it('a scene’s actions never offer another scene', async () => {
		const onchange = vi.fn();
		await render(ActionRow, {
			action: { action: 'nabaztag', command: 'ping' },
			index: 0,
			count: 1,
			sources: remoteSources(),
			onchange,
			onmove: vi.fn(),
			onremove: vi.fn(),
			types: SCENE_TYPES
		});
		await choice(m.remote_action_type()).click();
		await expect.element(page.getByRole('option', { name: ACTION_TYPES.tv_power() })).toBeVisible();
		await expect.element(page.getByRole('option', { name: ACTION_TYPES.scene() })).not.toBeInTheDocument();
	});
});
