import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { html } from '#lib/test/snippet.ts';
import Pad from './Pad.svelte';
import { PAD_KEY, type Fire, type PadKey } from './remote.ts';

function keys() {
	return Object.fromEntries((Object.keys(PAD_KEY) as PadKey[]).map((k) => [k, vi.fn<Fire>(async () => {})])) as Record<
		PadKey,
		ReturnType<typeof vi.fn<Fire>>
	>;
}

const LABEL = 'Pavé';

async function pad(props: Partial<{ volume: boolean; disabled: boolean }> = {}) {
	const k = keys();
	await render(Pad, { label: LABEL, keys: k, under: ['back', 'home'], ...props });
	const group = page.getByRole('group', { name: LABEL });
	const el = group.element() as HTMLElement;
	const press = (key: string, init: KeyboardEventInit = {}) =>
		el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }));
	return { k, group, el, press };
}

describe('Pad', () => {
	afterEach(() => vi.useRealTimers());

	it('is a named, focusable group described by its shortcuts', async () => {
		const { group, el } = await pad({ volume: true });
		await expect.element(group).toHaveAttribute('tabindex', '0');
		const list = document.getElementById(el.getAttribute('aria-describedby')!);
		expect(list?.textContent).toContain(m.tv_kbd_move());
		expect(list?.textContent).toContain(m.tv_mute());
	});

	it('lists the volume shortcuts only when it drives the volume', async () => {
		const { el } = await pad();
		const list = document.getElementById(el.getAttribute('aria-describedby')!);
		expect(list?.textContent).not.toContain(m.tv_kbd_m());
	});

	it('draws the arrows, OK and the keys under it, plus any extra key', async () => {
		await render(Pad, { label: LABEL, keys: keys(), under: ['back', 'home', 'menu'], extra: html('<button>Source</button>') });
		for (const name of [m.remote_keys_up(), m.remote_keys_left(), m.tv_key_ok(), m.remote_keys_right(), m.remote_keys_down(), m.tv_key_back(), m.nav_home(), m.remote_keys_menu(), 'Source'])
			await expect.element(page.getByRole('button', { name, exact: true })).toBeVisible();
	});

	it('while focused: arrows move, Enter is OK, Backspace is Back', async () => {
		const { k, el } = await pad();
		el.focus();
		await userEvent.keyboard('{ArrowUp}{ArrowLeft}{ArrowRight}{ArrowDown}{Enter}{Backspace}');
		for (const key of ['up', 'left', 'right', 'down', 'ok', 'back'] as const) expect(k[key]).toHaveBeenCalledExactlyOnceWith(false);
	});

	it('without `volume`, + and M do nothing', async () => {
		const plain = await pad();
		plain.press('+');
		plain.press('m');
		expect(plain.k.volume_up).not.toHaveBeenCalled();
		expect(plain.k.mute).not.toHaveBeenCalled();
	});

	it('with `volume`: + and = raise it, − lowers it, M mutes', async () => {
		const { k, press } = await pad({ volume: true });
		press('+');
		press('=');
		press('-');
		press('M');
		expect(k.volume_up).toHaveBeenCalledTimes(2);
		expect(k.volume_down).toHaveBeenCalledOnce();
		expect(k.mute).toHaveBeenCalledOnce();
	});

	it('on a focused key, Enter activates that key, not OK, and arrows do nothing', async () => {
		const { k } = await pad();
		(page.getByRole('button', { name: m.remote_keys_up() }).element() as HTMLElement).focus();
		await userEvent.keyboard('{Enter}{ArrowDown}');
		expect(k.up).toHaveBeenCalledExactlyOnceWith(false);
		expect(k.ok).not.toHaveBeenCalled();
		expect(k.down).not.toHaveBeenCalled();
	});

	it('ignores shortcuts with a modifier, and unknown keys', async () => {
		const { k, press } = await pad();
		press('ArrowUp', { ctrlKey: true });
		press('ArrowUp', { altKey: true });
		press('ArrowUp', { metaKey: true });
		press('x');
		expect(k.up).not.toHaveBeenCalled();
	});

	it('a disabled pad answers no shortcut', async () => {
		const off = await pad({ disabled: true });
		off.press('ArrowUp');
		expect(off.k.up).not.toHaveBeenCalled();
		await expect.element(page.getByRole('button', { name: m.remote_keys_up() })).toBeDisabled();
	});

	it('a held keyboard key repeats at the pad’s pace (100 ms arrows, 200 ms volume), never for OK', async () => {
		vi.useFakeTimers({ toFake: ['Date'] });
		vi.setSystemTime(10_000);
		const { k, press } = await pad({ volume: true });
		press('ArrowUp');
		press('ArrowUp', { repeat: true });
		expect(k.up).toHaveBeenCalledTimes(1);
		vi.setSystemTime(10_100);
		press('ArrowUp', { repeat: true });
		expect(k.up).toHaveBeenLastCalledWith(true);
		expect(k.up).toHaveBeenCalledTimes(2);

		press('+');
		vi.setSystemTime(10_250);
		press('+', { repeat: true });
		expect(k.volume_up).toHaveBeenCalledTimes(1);
		vi.setSystemTime(10_300);
		press('+', { repeat: true });
		expect(k.volume_up).toHaveBeenCalledTimes(2);

		press('Enter');
		vi.setSystemTime(20_000);
		press('Enter', { repeat: true });
		expect(k.ok).toHaveBeenCalledOnce();
	});
});
