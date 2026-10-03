import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ui } from './ui.svelte.ts';

describe('ui', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		ui.toasts = [];
	});
	afterEach(() => vi.useRealTimers());

	it('a toast is said politely and leaves after 5 s', async () => {
		ui.toast('Volet ajouté');
		expect(ui.toasts.map((t) => t.text)).toEqual(['Volet ajouté']);
		await vi.advanceTimersByTimeAsync(0);
		expect(ui.polite).toBe('Volet ajouté');
		await vi.advanceTimersByTimeAsync(5000);
		expect(ui.toasts).toEqual([]);
	});

	it('hover or focus holds a toast; leaving lets it go', async () => {
		ui.toast('Prise allumée');
		const id = ui.toasts[0].id;
		ui.pause(id);
		await vi.advanceTimersByTimeAsync(10_000);
		expect(ui.toasts).toHaveLength(1);
		ui.resume(id);
		await vi.advanceTimersByTimeAsync(5000);
		expect(ui.toasts).toHaveLength(0);
	});

	it('a failure is assertive and stays until dismissed', async () => {
		ui.fail(new Error('TV unreachable'));
		await vi.advanceTimersByTimeAsync(0);
		expect(ui.assertive).toContain('TV unreachable');
		await vi.advanceTimersByTimeAsync(60_000);
		expect(ui.toasts).toHaveLength(1);
		expect(ui.toasts[0].warn).toBe(true);
		ui.dismiss(ui.toasts[0].id);
		expect(ui.toasts).toHaveLength(0);
	});

	it('a failure says plain words, without « Erreur. » in front', async () => {
		ui.fail(new Error('Volet injoignable'));
		expect(ui.toasts[0].text).toBe('Volet injoignable');
	});

	it('the same failure twice does not stack two toasts', () => {
		ui.fail(new Error('Recherche impossible'));
		ui.fail(new Error('Recherche impossible'));
		expect(ui.toasts).toHaveLength(1);
	});

	it('remembers an explicit theme, forgets « system »', () => {
		ui.setTheme('dark');
		expect(localStorage.getItem('maison-theme')).toBe('dark');
		ui.setTheme('system');
		expect(localStorage.getItem('maison-theme')).toBeNull();
		expect(ui.theme).toBe('system');
	});
});
