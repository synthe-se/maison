import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync } from 'svelte';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import Key from './Key.svelte';

const pointer = (el: Element, type: string, button = 0) =>
	el.dispatchEvent(new PointerEvent(type, { button, pointerId: 1, bubbles: true, cancelable: true }));

describe('Key', () => {
	beforeEach(() => {
		// synthetic pointers have no active id to capture
		vi.spyOn(HTMLElement.prototype, 'setPointerCapture').mockImplementation(() => {});
	});
	afterEach(() => vi.useRealTimers());

	it('is named in words from the pad key, with its pressed state when it toggles', async () => {
		await render(Key, { k: 'mute', fire: vi.fn(), pressed: true });
		const key = page.getByRole('button', { name: m.tv_mute() });
		await expect.element(key).toHaveAttribute('aria-pressed', 'true');
		await expect.element(key).toHaveAttribute('title', m.tv_mute());
	});

	it('a free label wins over the pad key; no pressed state unless asked', async () => {
		await render(Key, { label: m.tv_key_source(), icon: 'monitor', fire: vi.fn() });
		const key = page.getByRole('button', { name: m.tv_key_source() });
		await expect.element(key).not.toHaveAttribute('aria-pressed');
	});

	it('sends on touch, then repeats an arrow after 500 ms every 100 ms, until released', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const fire = vi.fn();
		await render(Key, { k: 'up', fire });
		const el = page.getByRole('button', { name: m.remote_keys_up() }).element();
		pointer(el, 'pointerdown');
		expect(fire).toHaveBeenCalledExactlyOnceWith(false);
		flushSync();
		expect(el.classList.contains('down')).toBe(true);
		vi.advanceTimersByTime(499);
		expect(fire).toHaveBeenCalledTimes(1);
		vi.advanceTimersByTime(1);
		expect(fire).toHaveBeenLastCalledWith(true);
		vi.advanceTimersByTime(300);
		expect(fire).toHaveBeenCalledTimes(5);
		pointer(el, 'pointerup');
		vi.advanceTimersByTime(1000);
		expect(fire).toHaveBeenCalledTimes(5);
		await vi.advanceTimersByTimeAsync(0);
		expect(el.classList.contains('down')).toBe(false);
	});

	it('repeats the volume every 200 ms; a cancelled pointer stops it', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const fire = vi.fn();
		await render(Key, { k: 'volume_up', fire });
		const el = page.getByRole('button', { name: m.tv_volume_up() }).element();
		pointer(el, 'pointerdown');
		vi.advanceTimersByTime(500 + 400);
		expect(fire).toHaveBeenCalledTimes(4);
		pointer(el, 'pointercancel');
		vi.advanceTimersByTime(1000);
		expect(fire).toHaveBeenCalledTimes(4);
	});

	it('stays shown pressed at least 100 ms after a quick tap', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		await render(Key, { k: 'ok', fire: vi.fn() });
		const el = page.getByRole('button', { name: m.tv_key_ok() }).element();
		pointer(el, 'pointerdown');
		pointer(el, 'pointerup');
		flushSync();
		await vi.advanceTimersByTimeAsync(99);
		expect(el.classList.contains('down')).toBe(true);
		await vi.advanceTimersByTimeAsync(1);
		expect(el.classList.contains('down')).toBe(false);
	});

	it('never repeats OK, and ignores a secondary button', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const fire = vi.fn();
		await render(Key, { k: 'ok', fire });
		const el = page.getByRole('button', { name: m.tv_key_ok() }).element();
		pointer(el, 'pointerdown', 2);
		expect(fire).not.toHaveBeenCalled();
		pointer(el, 'pointerdown');
		vi.advanceTimersByTime(3000);
		expect(fire).toHaveBeenCalledExactlyOnceWith(false);
		pointer(el, 'pointerup');
	});

	it('a keyboard activation sends once; a pointer click does not send twice', async () => {
		const fire = vi.fn();
		await render(Key, { k: 'ok', fire });
		const key = page.getByRole('button', { name: m.tv_key_ok() });
		(key.element() as HTMLElement).focus();
		await userEvent.keyboard('{Enter}');
		expect(fire).toHaveBeenCalledExactlyOnceWith(false);
		key.element().dispatchEvent(new MouseEvent('click', { bubbles: true, detail: 1 }));
		expect(fire).toHaveBeenCalledOnce();
	});

	it('a disabled key sends nothing', async () => {
		const fire = vi.fn();
		await render(Key, { k: 'up', fire, disabled: true });
		const key = page.getByRole('button', { name: m.remote_keys_up() });
		await expect.element(key).toBeDisabled();
		pointer(key.element(), 'pointerdown');
		expect(fire).not.toHaveBeenCalled();
	});

	it('stops repeating when it leaves the page mid-hold', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
		const fire = vi.fn();
		const { unmount } = await render(Key, { k: 'down', fire });
		pointer(page.getByRole('button', { name: m.remote_keys_down() }).element(), 'pointerdown');
		await unmount();
		vi.advanceTimersByTime(2000);
		expect(fire).toHaveBeenCalledOnce();
	});
});
