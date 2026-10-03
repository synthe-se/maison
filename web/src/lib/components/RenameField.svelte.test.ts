import { afterEach, describe, expect, it, vi } from 'vitest';
import { page, userEvent } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { MAX_NAME } from '#lib/api.ts';
import { ui } from '#lib/ui.svelte.ts';
import { deferred } from '#lib/test/api.ts';
import RenameField from './RenameField.svelte';

const props = (over = {}) => ({ label: 'Nom', value: 'Salon', save: vi.fn(async () => {}), said: (n: string) => `Renommé en ${n}`, ...over });
const field = () => page.getByLabelText('Nom');

describe('RenameField', () => {
	afterEach(() => (ui.toasts = []));

	it('saves the trimmed name on Entrée, says it, and keeps the focus', async () => {
		const say = vi.spyOn(ui, 'say');
		const p = props();
		await render(RenameField, p);
		await field().fill('  Chambre ');
		await userEvent.keyboard('{Enter}');
		await expect.poll(() => p.save.mock.calls).toEqual([['Chambre']]);
		expect(say).toHaveBeenCalledWith('Renommé en Chambre');
		await expect.element(field()).toHaveFocus();
		await expect.element(field()).toHaveAttribute('maxlength', String(MAX_NAME));
	});

	it('Escape gives up: the name it had comes back', async () => {
		const p = props();
		await render(RenameField, p);
		await field().fill('Cha');
		await userEvent.keyboard('{Escape}');
		await expect.element(field()).toHaveValue('Salon');
		expect(p.save).not.toHaveBeenCalled();
	});

	it('an empty name is said under the field, nothing sent', async () => {
		const p = props();
		await render(RenameField, p);
		await field().fill('  ');
		await page.getByRole('button', { name: m.common_save() }).click();
		await expect.element(field()).toHaveAccessibleDescription(m.rename_empty());
		await expect.element(field()).toHaveFocus();
		expect(p.save).not.toHaveBeenCalled();
	});

	it('a refusal is said under the field; the draft stays', async () => {
		const p = props({ save: vi.fn(async () => Promise.reject(new Error('Nom trop long'))) });
		await render(RenameField, p);
		await field().fill('Chambre');
		await userEvent.keyboard('{Enter}');
		await expect.element(field()).toHaveAccessibleDescription('Nom trop long');
		await expect.element(field()).toHaveValue('Chambre');
		await expect.element(field()).toHaveFocus();
	});

	it('busy while it saves: said so, still focusable, a second press sends nothing more', async () => {
		const d = deferred();
		const p = props({ save: vi.fn(() => d.promise) });
		await render(RenameField, p);
		await field().fill('Chambre');
		const save = page.getByRole('button', { name: m.common_save() });
		await save.click();
		await expect.element(save).toHaveAttribute('aria-busy', 'true');
		(save.element() as HTMLElement).click();
		d.resolve(undefined);
		expect(p.save).toHaveBeenCalledOnce();
	});

	it('inline: closes on save or Cancel (the caller puts the focus back)', async () => {
		const ondone = vi.fn();
		await render(RenameField, props({ ondone, autofocus: true }));
		await expect.element(field()).toHaveFocus();
		await page.getByRole('button', { name: m.common_cancel() }).click();
		expect(ondone).toHaveBeenCalledOnce();
	});
});
