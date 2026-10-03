import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { IrAction, IrBinding, IrEvent } from '#lib/api.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred, stubApi, type ApiCall } from '#lib/test/api.ts';
import { remoteSources } from '#lib/test/remote.ts';
import BindingEditor from './BindingEditor.svelte';
import { summarize } from './actions.ts';
import { keyName } from './keys.ts';

const nab = (command: string): IrAction => ({ action: 'nabaztag', command });
const commands = () => page.getByRole('textbox', { name: m.remote_fields_command() });
const save = () => page.getByRole('button', { name: m.common_save() });
const test = () => page.getByRole('button', { name: m.remote_test() });
const codeBox = () => page.getByRole('textbox', { name: m.remote_key_code() });

async function editor(code: number | undefined, keymap: Record<string, IrBinding> = {}) {
	const onsaved = vi.fn();
	const oncancel = vi.fn();
	await render(BindingEditor, { code, keymap, sources: remoteSources(), onsaved, oncancel });
	return { onsaved, oncancel };
}

/** The STB's recent events: nothing at first, then `press` (newer than the baseline). */
function remote(press: Partial<IrEvent> | null) {
	let asked = 0;
	return () => {
		asked++;
		const old: IrEvent = { seq: 7, code: 2, value: 1, mapped: false, receivedAt: '2026-10-02T10:00:00Z' };
		return { success: true, events: asked > 1 && press ? [{ ...old, seq: 8, receivedAt: '2026-10-02T10:00:05Z', ...press }, old] : [old] };
	};
}

describe('BindingEditor', () => {
	afterEach(() => {
		ui.toasts = [];
	});

	it('a new button is captured from the remote: the first press after it started', async () => {
		stubApi({ '/ir/recent': remote({ code: 115 }) });
		const say = vi.spyOn(ui, 'say');
		await editor(undefined);
		// a plain button whose words change (APG: no aria-pressed on such a button)
		await expect.element(page.getByRole('button', { name: m.remote_stop_capture() })).not.toHaveAttribute('aria-pressed');
		await expect.element(page.getByText(m.remote_capturing())).toBeVisible();
		await expect.element(codeBox(), { timeout: 3000 }).toHaveValue('115');
		await expect.element(page.getByText(keyName(115), { exact: true })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.remote_capture() })).toBeVisible();
		expect(say).toHaveBeenCalledWith(m.remote_captured({ key: keyName(115) }));
	});

	it('ignores a release or an autorepeat while capturing', async () => {
		const api = stubApi({ '/ir/recent': remote({ code: 115, value: 0 }) });
		await editor(undefined);
		await expect.poll(() => api.calls.length, { timeout: 3000 }).toBeGreaterThanOrEqual(2);
		await expect.element(codeBox()).toHaveValue('');
		await page.getByRole('button', { name: m.remote_stop_capture() }).click();
		await expect.element(page.getByRole('button', { name: m.remote_capture() })).toBeVisible();
	});

	it('a captured key already configured brings its binding, and says so', async () => {
		stubApi({ '/ir/recent': remote({ code: 353 }) });
		await editor(undefined, { '353': { label: 'Taichi', actions: [nab('chor taichi')] } });
		await expect.element(page.getByLabelText(m.remote_label()), { timeout: 3000 }).toHaveValue('Taichi');
		await expect.element(commands()).toHaveValue('chor taichi');
		await expect.element(page.getByText(m.remote_already_mapped())).toBeVisible();
	});

	it('stops capturing when the set-top box cannot be read, and says why', async () => {
		stubApi({ '/ir/recent': new Response('{"error":"STB unreachable"}', { status: 502 }) });
		const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
		await editor(undefined);
		await expect.poll(() => fail).toHaveBeenCalledOnce();
		await expect.element(page.getByRole('button', { name: m.remote_capture() })).toBeVisible();
	});

	it('captures by the backend’s sequence, not its clock (an NTP step back hides nothing)', async () => {
		let asked = 0;
		const old: IrEvent = { seq: 7, code: 2, value: 1, mapped: false, receivedAt: '2026-10-02T10:00:00Z' };
		// the new press is stamped earlier than the old one: the clock went back
		stubApi({ '/ir/recent': () => ({ success: true, events: ++asked > 1 ? [{ ...old, seq: 8, code: 115, receivedAt: '2026-10-02T09:59:00Z' }, old] : [old] }) });
		await editor(undefined);
		await expect.element(codeBox(), { timeout: 3000 }).toHaveValue('115');
	});

	it('after a backend restart (the sequence starts again), the new press is still captured', async () => {
		let asked = 0;
		const old: IrEvent = { seq: 40, code: 2, value: 1, mapped: false, receivedAt: '2026-10-02T10:00:00Z' };
		stubApi({ '/ir/recent': () => ({ success: true, events: ++asked > 1 ? [{ ...old, seq: 1, code: 116 }] : [old] }) });
		await editor(undefined);
		await expect.element(codeBox(), { timeout: 3000 }).toHaveValue('116');
	});

	it('takes a typed code instead, and names its key', async () => {
		stubApi({ '/ir/recent': () => new Promise(() => {}) });
		await editor(undefined, { '102': { actions: [nab('ping')], repeat: true } });
		await codeBox().fill('102');
		await expect.element(page.getByText(keyName(102), { exact: true })).toBeVisible();
		await expect.element(page.getByText(m.remote_capturing())).not.toBeInTheDocument();
		await expect.element(commands()).toHaveValue('ping');
		await expect.element(page.getByRole('switch', { name: m.remote_repeat() })).toBeChecked();
	});

	it('does not replace what was already typed with a configured key’s binding', async () => {
		stubApi({ '/ir/recent': () => new Promise(() => {}) });
		await editor(undefined, { '102': { label: 'Accueil', actions: [nab('ping')] } });
		await page.getByLabelText(m.remote_label()).fill('Mon bouton');
		await codeBox().fill('102');
		await expect.element(page.getByLabelText(m.remote_label())).toHaveValue('Mon bouton');
		await expect.element(page.getByText(m.remote_already_mapped())).toBeVisible();
	});

	it('a configured key edits its binding: its name, number, label and actions', async () => {
		await editor(116, { '116': { label: 'Tout éteindre', repeat: false, actions: [nab('stop')] } });
		await expect.element(page.getByText(keyName(116), { exact: false }).first()).toBeVisible();
		await expect.element(page.getByText(m.remote_key_number({ code: 116 }))).toBeVisible();
		await expect.element(page.getByLabelText(m.remote_label())).toHaveValue('Tout éteindre');
		await expect.element(commands()).toHaveValue('stop');
		await expect.element(codeBox()).not.toBeInTheDocument();
	});

	it('reorders the actions with up/down, the focus following the action', async () => {
		const say = vi.spyOn(ui, 'say');
		await editor(116, { '116': { actions: [nab('a'), nab('b'), nab('c')] } });
		await page.getByRole('button', { name: m.remote_move_down({ n: 1 }) }).click();
		await expect.element(commands().nth(0)).toHaveValue('b');
		await expect.element(commands().nth(1)).toHaveValue('a');
		await expect.element(page.getByRole('button', { name: m.remote_move_down({ n: 2 }) })).toHaveFocus();
		expect(say).toHaveBeenCalledWith(m.remote_moved({ n: 2 }));
		// to the end: the focus goes to the arrow still usable
		await page.getByRole('button', { name: m.remote_move_down({ n: 2 }) }).click();
		await expect.element(commands().nth(2)).toHaveValue('a');
		await expect.element(page.getByRole('button', { name: m.remote_move_up({ n: 3 }) })).toHaveFocus();
		await page.getByRole('button', { name: m.remote_move_up({ n: 2 }) }).click();
		await expect.element(commands().nth(0)).toHaveValue('c');
		await expect.element(page.getByRole('button', { name: m.remote_move_down({ n: 1 }) })).toHaveFocus();
		await page.getByRole('button', { name: m.remote_move_up({ n: 3 }) }).click();
		await expect.element(page.getByRole('button', { name: m.remote_move_up({ n: 2 }) })).toHaveFocus();
		await expect.element(commands().nth(1)).toHaveValue('a');
	});

	it('removes an action, and puts the focus on « add »; adds one', async () => {
		const say = vi.spyOn(ui, 'say');
		await editor(116, { '116': { actions: [nab('a'), nab('b')] } });
		await page.getByRole('button', { name: m.remote_remove_action({ n: 1 }) }).click();
		await expect.element(commands()).toHaveLength(1);
		await expect.element(page.getByRole('button', { name: m.remote_add_action() })).toHaveFocus();
		expect(say).toHaveBeenCalledWith(m.remote_action_removed());
		await page.getByRole('button', { name: m.remote_add_action() }).click();
		await expect.element(commands().nth(1)).toHaveValue('');
	});

	it('will not test nor save without actions, or with one incomplete, and says why', async () => {
		const api = stubApi({});
		await editor(116, { '116': { actions: [] } });
		await expect.element(page.getByText(m.remote_missing_actions())).toBeVisible();
		await test().click();
		await expect.element(page.getByText(m.remote_missing_actions()).nth(1)).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.remote_add_action() })).toHaveAccessibleDescription(m.remote_missing_actions());
		await page.getByRole('button', { name: m.remote_add_action() }).click();
		await save().click();
		await expect.element(page.getByText(m.remote_incomplete_actions())).toBeVisible();
		expect(api.calls).toEqual([]);
	});

	it('will not save without a key code', async () => {
		stubApi({ '/ir/recent': () => new Promise(() => {}) });
		await editor(undefined);
		await save().click();
		await expect.element(page.getByText(m.remote_missing_code())).toBeVisible();
		// tied to its field, which takes the focus
		await expect.element(codeBox()).toHaveAccessibleDescription(m.remote_missing_code());
		await expect.element(codeBox()).toHaveFocus();
	});

	it('tests the actions now, and says how each one went, in words', async () => {
		const answer = deferred();
		const api = stubApi({ 'POST /ir/test': () => answer.promise });
		const say = vi.spyOn(ui, 'say');
		const actions: IrAction[] = [nab('ping'), { action: 'meross_power', device: 'p1', state: 'on' }];
		await editor(116, { '116': { actions } });
		await test().click();
		await expect.element(page.getByText(m.remote_testing())).toBeVisible();
		await expect.element(test()).toBeDisabled();
		answer.resolve({ success: false, message: '', results: ['ok: sent', 'failed: plug offline'] });
		const results = page.getByRole('listitem').filter({ hasText: m.remote_test_item_ok() });
		await expect.element(results).toMatchTextContent(summarize(actions[0], remoteSources()));
		const failed = page.getByRole('listitem').filter({ hasText: m.remote_test_item_failed() });
		await expect.element(failed).toMatchTextContent(summarize(actions[1], remoteSources()));
		await expect.element(failed).toMatchTextContent('plug offline');
		await expect.element(page.getByText(m.remote_test_failed())).toBeVisible();
		expect(say).toHaveBeenCalledWith(m.remote_test_failed());
		expect((api.sent('POST', '/ir/test')[0] as ApiCall).body).toEqual({ actions });
		// an edit makes the outcome stale
		await commands().fill('pong');
		await expect.element(page.getByText(m.remote_test_failed())).not.toBeInTheDocument();
	});

	it('says a passed test', async () => {
		stubApi({ 'POST /ir/test': { success: true, message: '', results: ['ok: done'] } });
		await editor(116, { '116': { actions: [nab('ping')] } });
		await test().click();
		await expect.element(page.getByText(m.remote_test_passed())).toBeVisible();
	});

	it('saves the whole binding, keeping the actions it does not edit and debounce_ms', async () => {
		const api = stubApi({ 'PUT /ir/keymap/116': { success: true, message: '' } });
		const tv = { action: 'tv_power', state: 'standby' } as unknown as IrAction;
		const binding = { label: 'TV', actions: [tv, nab('ping')], debounce_ms: 300 } as IrBinding;
		const { onsaved } = await editor(116, { '116': binding });
		await page.getByLabelText(m.remote_label()).fill('  ');
		await page.getByRole('switch', { name: m.remote_repeat() }).click();
		await save().click();
		await expect.poll(() => onsaved).toHaveBeenCalledOnce();
		expect(api.sent('PUT', '/ir/keymap/116')[0].body).toEqual({ actions: [tv, nab('ping')], debounce_ms: 300, repeat: true });
		expect(ui.toasts.map((t) => t.text)).toContain(m.remote_saved({ key: keyName(116) }));
	});

	it('saves a typed key with its label trimmed', async () => {
		const api = stubApi({ '/ir/recent': () => new Promise(() => {}), 'PUT /ir/keymap/412': { success: true, message: '' } });
		const { onsaved } = await editor(undefined);
		await codeBox().fill('412');
		await page.getByLabelText(m.remote_label()).fill(' Lumière ');
		await page.getByRole('button', { name: m.remote_add_action() }).click();
		await commands().fill('ping');
		await save().click();
		await expect.poll(() => onsaved).toHaveBeenCalledOnce();
		expect(api.sent('PUT', '/ir/keymap/412')[0].body).toEqual({ actions: [nab('ping')], label: 'Lumière', repeat: false });
	});

	it('cancels', async () => {
		const { oncancel } = await editor(116, { '116': { actions: [] } });
		await page.getByRole('button', { name: m.common_cancel() }).click();
		expect(oncancel).toHaveBeenCalledOnce();
	});
});
