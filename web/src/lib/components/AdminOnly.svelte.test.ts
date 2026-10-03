import { afterEach, describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { html } from '#lib/test/snippet.ts';
import AdminOnly from './AdminOnly.svelte';

const settings = html('<button>Réglages</button>');

describe('AdminOnly', () => {
	afterEach(() => {
		session.user = null;
		session.status = 'loading';
	});

	it('an admin sees what it holds', async () => {
		session.adopt(leonard);
		await render(AdminOnly, { children: settings });
		await expect.element(page.getByRole('button', { name: 'Réglages' })).toBeVisible();
	});

	it('a member sees the short reason instead', async () => {
		session.adopt(alex);
		await render(AdminOnly, { children: settings });
		await expect.element(page.getByRole('button', { name: 'Réglages' })).not.toBeInTheDocument();
		await expect.element(page.getByText(m.admin_only())).toBeVisible();
	});

	it('or nothing at all when there is nothing to explain', async () => {
		session.adopt(alex);
		const { container } = await render(AdminOnly, { reason: false, children: settings });
		expect(container.textContent?.trim()).toBe('');
	});
});
