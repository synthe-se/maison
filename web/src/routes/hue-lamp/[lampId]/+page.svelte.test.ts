import { afterEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { forgetAll } from '#lib/live.svelte.ts';
import { stubApi } from '#lib/test/api.ts';
import DetailPage from './+page.svelte';

// the route's parameters, as SvelteKit would give them
const route = vi.hoisted(() => ({ params: {} as Record<string, string | undefined> }));
vi.mock('$app/state', () => ({ page: route }));
vi.mock('$app/navigation', () => ({ goto: vi.fn(async () => {}) }));

describe('Hue lamp page', () => {
	afterEach(() => forgetAll());

	it('shows the Hue lamp named in the address', async () => {
		route.params = { lampId: 'hue-7' };
		const api = stubApi({});
		await render(DetailPage);
		await expect.poll(() => api.calls.map((c) => c.path)).toContain('/hue-lamps/hue-7');
	});

	it('asks for no particular one when the address names none', async () => {
		route.params = {};
		const api = stubApi({});
		await render(DetailPage);
		await expect.poll(() => api.calls.length).toBeGreaterThan(0);
		expect(api.calls.map((c) => c.path)).not.toContain('/hue-lamps/hue-7');
	});
});
