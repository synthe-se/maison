import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { ui } from '#lib/ui.svelte.ts';
import Range from './Range.svelte';

const valueText = (v: number) => `Ouvert à ${v} %`;
const base = { label: 'Volet du salon', value: 50, valueText };

const thumb = () => document.querySelector<HTMLElement>('[role=slider]')!;
const said = () => thumb().getAttribute('aria-valuetext');

/** A pointer gesture on the band: down at the first position (0–1 of the track), moves, release. */
function pointer(type: string, at?: number) {
	const band = document.querySelector<HTMLElement>('.range-band')!;
	const { left, width, top, height } = band.getBoundingClientRect();
	const init = { bubbles: true, button: 0, clientX: left + (at ?? 0) * width, clientY: top + height / 2, pointerId: 1 };
	(type === 'pointerdown' ? band : document).dispatchEvent(new PointerEvent(type, init));
	flushSync();
}
const press = (at: number) => pointer('pointerdown', at);
const move = (at: number) => pointer('pointermove', at);
const release = () => pointer('pointerup');

/** Lets the sends and the settle resolve. */
const tick = () => vi.advanceTimersByTimeAsync(0);

describe('Range', () => {
	afterEach(() => vi.useRealTimers());

	it('is a slider named by its label, its value said in words', async () => {
		await render(Range, { ...base, ends: ['Fermé', 'Ouvert'] });
		const slider = page.getByRole('slider', { name: 'Volet du salon' });
		await expect.element(slider).toHaveAttribute('aria-valuenow', '50');
		await expect.element(slider).toHaveAttribute('aria-valuetext', 'Ouvert à 50 %');
		await expect.element(page.getByText('Ouvert à 50 %')).toBeVisible();
		await expect.element(page.getByText('Fermé')).toBeVisible();
	});

	it('can keep its label for readers only, and be disabled', async () => {
		await render(Range, { ...base, hideLabel: true, disabled: true });
		const slider = page.getByRole('slider', { name: 'Volet du salon' });
		await expect.element(slider).toHaveAttribute('aria-disabled', 'true');
		expect(document.querySelector('.head')!.classList.contains('sr-only')).toBe(true);
	});

	describe('a form value', () => {
		it('commits on release only, not while dragging', async () => {
			const oncommit = vi.fn();
			await render(Range, { ...base, oncommit });
			press(0.2);
			move(0.3);
			move(0.4);
			expect(oncommit).not.toHaveBeenCalled();
			expect(said()).toBe('Ouvert à 40 %');
			release();
			expect(oncommit).toHaveBeenCalledExactlyOnceWith(40);
		});

		it('commits on each key press', async () => {
			const oncommit = vi.fn();
			await render(Range, { ...base, oncommit });
			thumb().focus();
			await userEvent.keyboard('{ArrowRight}{ArrowRight}');
			expect(oncommit.mock.calls).toEqual([[51], [52]]);
			expect(said()).toBe('Ouvert à 52 %');
		});

		it('moves a page of steps with Page Up / Page Down, within its bounds', async () => {
			const oncommit = vi.fn();
			await render(Range, { ...base, value: 95, step: 5, page: 2, oncommit });
			thumb().focus();
			await userEvent.keyboard('{PageDown}');
			expect(oncommit).toHaveBeenLastCalledWith(85);
			expect(said()).toBe('Ouvert à 85 %');
			await userEvent.keyboard('{PageUp}{PageUp}');
			expect(oncommit).toHaveBeenLastCalledWith(100);
			await userEvent.keyboard('{Home}');
			await userEvent.keyboard('{PageDown}');
			expect(oncommit).toHaveBeenLastCalledWith(0);
		});

		it('Page keys do nothing when disabled', async () => {
			const oncommit = vi.fn();
			await render(Range, { ...base, oncommit, disabled: true });
			thumb().dispatchEvent(new KeyboardEvent('keydown', { key: 'PageUp', bubbles: true }));
			flushSync();
			expect(oncommit).not.toHaveBeenCalled();
			expect(said()).toBe('Ouvert à 50 %');
		});
	});

	describe('a device', () => {
		it('sends on release only, then reads the device again', async () => {
			vi.useFakeTimers();
			const order: string[] = [];
			const send = vi.fn(async (v: number) => void order.push(`send ${v}`));
			const settle = vi.fn(async () => void order.push('settle'));
			await render(Range, { ...base, send, settle });
			press(0.2);
			move(0.3);
			await tick();
			expect(send).not.toHaveBeenCalled();
			release();
			await tick();
			expect(order).toEqual(['send 30', 'settle']);
		});

		it('live: one send per 300 ms while dragging, and always the last value', async () => {
			vi.useFakeTimers();
			const send = vi.fn(async (_v: number) => {});
			await render(Range, { ...base, send, live: true });
			press(0.1);
			await tick();
			expect(send.mock.calls).toEqual([[10]]);
			move(0.2);
			move(0.3);
			move(0.35);
			await vi.advanceTimersByTimeAsync(299);
			expect(send).toHaveBeenCalledTimes(1);
			await vi.advanceTimersByTimeAsync(1);
			expect(send.mock.calls).toEqual([[10], [35]]);
			move(0.6);
			release();
			await vi.advanceTimersByTimeAsync(300);
			expect(send.mock.calls).toEqual([[10], [35], [60]]);
			await vi.advanceTimersByTimeAsync(1000);
			expect(send).toHaveBeenCalledTimes(3);
		});

		it('does not send again a value the device was just sent', async () => {
			vi.useFakeTimers();
			const send = vi.fn(async (_v: number) => {});
			const settle = vi.fn(async () => {});
			await render(Range, { ...base, send, settle, live: true });
			press(0.7);
			await vi.advanceTimersByTimeAsync(300);
			release();
			await vi.advanceTimersByTimeAsync(300);
			expect(send.mock.calls).toEqual([[70]]);
			expect(settle).toHaveBeenCalledOnce();
		});

		it('holds the sent value until the device reports it', async () => {
			vi.useFakeTimers();
			const send = vi.fn(async (_v: number) => {});
			const { rerender } = await render(Range, { ...base, send, near: 2 });
			press(0.8);
			release();
			await tick();
			// the device still says 50: the thumb does not jump back
			await rerender({ value: 50 });
			expect(said()).toBe('Ouvert à 80 %');
			// close enough counts as reached: the device's value wins
			await rerender({ value: 79 });
			expect(said()).toBe('Ouvert à 79 %');
		});

		it('gives up holding after 3 s of a device that does not follow', async () => {
			vi.useFakeTimers();
			const send = vi.fn(async (_v: number) => {});
			await render(Range, { ...base, send });
			press(0.8);
			release();
			await tick();
			await vi.advanceTimersByTimeAsync(2999);
			flushSync();
			expect(said()).toBe('Ouvert à 80 %');
			await vi.advanceTimersByTimeAsync(1);
			flushSync();
			expect(said()).toBe('Ouvert à 50 %');
		});

		it('Page Up sends a page of steps at once', async () => {
			vi.useFakeTimers();
			const send = vi.fn(async (_v: number) => {});
			await render(Range, { ...base, send, page: 10 });
			thumb().dispatchEvent(new KeyboardEvent('keydown', { key: 'PageUp', bubbles: true, cancelable: true }));
			await tick();
			expect(send).toHaveBeenCalledExactlyOnceWith(60);
		});

		it('a failed send is said, and the thumb goes back to the device', async () => {
			vi.useFakeTimers();
			const fail = vi.spyOn(ui, 'fail').mockImplementation(() => {});
			const error = new Error('Volet injoignable');
			const send = vi.fn(async (_v: number) => {
				throw error;
			});
			await render(Range, { ...base, send });
			press(0.9);
			release();
			await tick();
			flushSync();
			expect(fail).toHaveBeenCalledExactlyOnceWith(error);
			expect(said()).toBe('Ouvert à 50 %');
		});

		it('after a failure, the same value can be sent again', async () => {
			vi.useFakeTimers();
			vi.spyOn(ui, 'fail').mockImplementation(() => {});
			const send = vi.fn().mockRejectedValueOnce(new Error('no')).mockResolvedValue(undefined);
			await render(Range, { ...base, send });
			press(0.9);
			release();
			await tick();
			press(0.9);
			release();
			await tick();
			expect(send.mock.calls).toEqual([[90], [90]]);
		});
	});
});
