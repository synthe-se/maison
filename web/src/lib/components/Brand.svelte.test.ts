import { describe, expect, it } from 'vitest';
import { page } from 'vitest/browser';
import { render } from 'vitest-browser-svelte';
import { m } from '#lib/paraglide/messages.js';
import Brand from './Brand.svelte';

describe('Brand', () => {
	it('is one link home, named once for readers', async () => {
		await render(Brand);
		const link = page.getByRole('link', { name: m.branding_name() });
		await expect.element(link).toHaveAttribute('href', '/');
		// the logo is decorative: the link's name is not said twice
		await expect.element(link.getByRole('img')).not.toBeInTheDocument();
	});
});
