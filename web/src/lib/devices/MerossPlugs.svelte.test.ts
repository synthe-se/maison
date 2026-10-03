import { afterEach, describe, expect, it } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { forgetAll } from '#lib/live.svelte.ts';
import { json, stubFetch } from '#lib/test/fetch.ts';
import { merossElectricity, merossPlug } from '#lib/test/meross.ts';
import MerossPlugs from './MerossPlugs.svelte';

afterEach(() => forgetAll());

const plug = (id: string, isOnline: boolean, isOn = false, lastPing = 0) => merossPlug({ id, name: `Prise ${id}`, isOnline, isOn, lastPing });

describe('MerossPlugs', () => {
	it('says it is loading, then one tile per plug, the counts in the head', async () => {
		let answer!: (r: Response) => void;
		const calls = stubFetch((url) => {
			if (url === '/api/meross') return new Promise<Response>((r) => (answer = r));
			return json(merossElectricity('a'));
		});
		await render(MerossPlugs);
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
		answer(json({ success: true, devices: [plug('a', true, true), plug('b', false)], total: 2 }));
		await expect.element(page.getByRole('button', { name: 'Prise a' })).toHaveAttribute('aria-pressed', 'true');
		await expect.element(page.getByText(`${m.meross_plug_count({ count: 2 })} · ${m.meross_online_count({ count: 1 })}`)).toBeVisible();
		// only the plug that answers is asked for its power
		await expect.poll(() => calls.map((c) => c.url).sort()).toEqual(['/api/meross', '/api/meross/a/electricity']);
		await expect.element(page.getByText(m.state_never_seen())).toBeVisible();
	});

	it('an unreachable plug keeps its gesture, unavailable, and says since when', async () => {
		stubFetch(() => json({ success: true, devices: [plug('b', false, false, Date.now() - 4 * 60_000)], total: 1 }));
		await render(MerossPlugs);
		await expect.element(page.getByRole('button', { name: 'Prise b' })).toHaveAttribute('aria-disabled', 'true');
		await expect.element(page.getByText(m.state_unreachable_for({ duration: m.duration_minutes({ m: 4 }) }))).toBeVisible();
	});

	it('a list that cannot be read says so, never « no plug »', async () => {
		stubFetch(() => json({ success: false, error: 'down' }, 500));
		await render(MerossPlugs);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByText(m.meross_none())).not.toBeInTheDocument();
	});

	it('no plug: says so and how they appear', async () => {
		stubFetch(() => json({ success: true, devices: [], total: 0 }));
		await render(MerossPlugs);
		await expect.element(page.getByText(m.meross_none())).toBeVisible();
		await expect.element(page.getByText(m.meross_none_hint())).toBeVisible();
	});
});
