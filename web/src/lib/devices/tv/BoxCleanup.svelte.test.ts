import { describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { ui } from '#lib/ui.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import type { CleanupResponse } from './api.ts';
import BoxCleanup from './BoxCleanup.svelte';

function cleanup(over: Partial<CleanupResponse> = {}): CleanupResponse {
	return {
		success: true,
		launcher: true,
		packages: [
			{ package: 'com.oemservice', why: 'vendor_agent', state: 'kept' },
			{ package: 'com.oem.qa', why: 'vendor_agent', state: 'cleaned' },
			{ package: 'com.netflix.ninja', why: 'unused_app', state: 'absent' }
		],
		...over
	};
}

describe('tv/BoxCleanup', () => {
	it('says where the box stands, leaving out what it never had', async () => {
		stubApi({ 'GET /androidtv/cleanup': cleanup() });
		await render(BoxCleanup);
		await expect.element(page.getByText(m.box_clean_count({ cut: 1, total: 2 }))).toBeVisible();
		await expect.element(page.getByText(m.box_clean_why_vendor_agent())).toBeVisible();
		await expect.element(page.getByText(m.box_clean_why_unused_app())).not.toBeInTheDocument();
	});

	it('cleans, then shows what the box answers', async () => {
		const toast = vi.spyOn(ui, 'toast');
		const done = cleanup({
			packages: [
				{ package: 'com.oemservice', why: 'vendor_agent', state: 'cleaned' },
				{ package: 'com.oem.qa', why: 'vendor_agent', state: 'cleaned' }
			]
		});
		const api = stubApi({ 'GET /androidtv/cleanup': cleanup(), 'POST /androidtv/cleanup': done });
		await render(BoxCleanup);
		await page.getByRole('button', { name: m.box_clean_action() }).click();
		await expect.element(page.getByText(m.box_clean_count({ cut: 2, total: 2 }))).toBeVisible();
		expect(toast).toHaveBeenCalledWith(m.box_clean_done());
		expect(JSON.stringify(api.calls)).toContain('clean');
	});

	it('cannot clean without the home that replaces Google’s, and says why; restoring still can', async () => {
		stubApi({ 'GET /androidtv/cleanup': cleanup({ launcher: false }) });
		await render(BoxCleanup);
		const clean = page.getByRole('button', { name: m.box_clean_action() });
		await expect.element(clean).toHaveAccessibleDescription(m.box_clean_no_home());
		await expect.element(page.getByRole('button', { name: m.box_clean_restore() })).not.toHaveAttribute('aria-disabled', 'true');
	});
});
