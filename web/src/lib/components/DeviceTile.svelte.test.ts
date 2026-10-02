import { afterEach, describe, expect, it, vi } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { flushSync } from 'svelte';
import { m } from '#lib/paraglide/messages.js';
import { Command } from '#lib/command.svelte.ts';
import { deferred } from '#lib/test/api.ts';
import { text } from '#lib/test/snippet.ts';
import DeviceTile from './DeviceTile.svelte';

const base = { name: 'Lampe du salon', icon: 'lightbulb' as const, state: 'Allumée, 80 %' };

describe('DeviceTile', () => {
	afterEach(() => vi.useRealTimers());

	it('is an article named after the device, its state in words, its fact on the side', async () => {
		await render(DeviceTile, { ...base, fact: '42 W', href: '/hue-lamp/1' });
		await expect.element(page.getByRole('article', { name: base.name })).toBeVisible();
		await expect.element(page.getByRole('link', { name: base.name })).toHaveAttribute('href', '/hue-lamp/1');
		await expect.element(page.getByText(base.state)).toBeVisible();
		await expect.element(page.getByText('42 W')).toBeVisible();
	});

	it('its name is a heading, h3 under a group, h2 alone on a page', async () => {
		const { rerender } = await render(DeviceTile, base);
		await expect.element(page.getByRole('heading', { level: 3, name: base.name }), { timeout: 500 }).toBeVisible();
		await rerender({ level: 2 });
		await expect.element(page.getByRole('heading', { level: 2, name: base.name }), { timeout: 500 }).toBeVisible();
	});

	it('without a page or a gesture: a plain name, a decorative icon', async () => {
		await render(DeviceTile, { ...base, on: true, warn: true, state: 'Injoignable depuis 3 min' });
		await expect.element(page.getByRole('link')).not.toBeInTheDocument();
		await expect.element(page.getByRole('button')).not.toBeInTheDocument();
		await expect.element(page.getByText('Injoignable depuis 3 min')).toHaveClass('warn');
	});

	it('its icon is a pressed button named after the device, asking for the other state', async () => {
		const ontoggle = vi.fn();
		await render(DeviceTile, { ...base, on: true, ontoggle });
		const button = page.getByRole('button', { name: base.name });
		await expect.element(button).toHaveAttribute('aria-pressed', 'true');
		await button.click();
		expect(ontoggle).toHaveBeenCalledExactlyOnceWith(false);
	});

	it('draws its extra controls and end buttons', async () => {
		await render(DeviceTile, { ...base, end: text('Réglages'), children: text('Luminosité') });
		await expect.element(page.getByText('Réglages')).toBeVisible();
		await expect.element(page.getByText('Luminosité')).toBeVisible();
	});

	it('follows a command: the target at once, « Allumage… » after 1 s, the state again on answer', async () => {
		vi.useFakeTimers();
		const command = new Command(() => base.name, 3000);
		const { container } = await render(DeviceTile, { ...base, state: 'Éteinte', on: false, command, ontoggle: () => {} });
		const button = () => container.querySelector('button.gesture')!;
		const line = () => container.querySelector('.tile-state')!.textContent!.trim();
		const send = deferred();
		const run = command.run(true, () => send.promise);
		flushSync();
		expect(button().getAttribute('aria-pressed')).toBe('true');
		expect(line()).toBe('Éteinte');
		await vi.advanceTimersByTimeAsync(1000);
		flushSync();
		expect(line()).toBe(m.command_turning_on());
		send.resolve(undefined);
		await run;
		flushSync();
		expect(line()).toBe('Éteinte');
		expect(button().getAttribute('aria-pressed')).toBe('false');
	});

	it('says « Extinction… » when turning off', async () => {
		vi.useFakeTimers();
		const command = new Command(() => base.name, 3000);
		const { container } = await render(DeviceTile, { ...base, on: true, command, ontoggle: () => {} });
		void command.run(false, () => deferred().promise);
		await vi.advanceTimersByTimeAsync(1000);
		flushSync();
		expect(container.querySelector('.tile-state')!.textContent!.trim()).toBe(m.command_turning_off());
	});

	it('with no answer: « Pas de réponse » and a retry asking the same again', async () => {
		vi.useFakeTimers();
		const ontoggle = vi.fn();
		const command = new Command(() => base.name, 3000);
		const { container } = await render(DeviceTile, { ...base, on: false, command, ontoggle });
		const run = command.run(true, () => deferred().promise);
		await vi.advanceTimersByTimeAsync(3000);
		await run;
		flushSync();
		const state = container.querySelector('.tile-state')!;
		expect(state.textContent).toContain(m.command_no_answer_short());
		expect(state.classList.contains('warn')).toBe(true);
		expect(container.querySelector('button.gesture')!.getAttribute('aria-pressed')).toBe('false');
		const retry = [...state.querySelectorAll('button')].find((b) => b.textContent === m.common_retry())!;
		retry.click();
		expect(ontoggle).toHaveBeenCalledExactlyOnceWith(true);
	});
});
