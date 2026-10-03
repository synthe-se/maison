import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import type { Shutter } from '#lib/api.ts';
import { forgetAll } from '#lib/live.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { json, sentBody, stubFetch, type FetchCall } from '#lib/test/fetch.ts';
import { shutter } from '#lib/test/shutters.ts';
import Shutters from './Shutters.svelte';

beforeEach(() => session.adopt(leonard));
afterEach(() => {
	vi.useRealTimers();
	forgetAll();
});


/** The Matter API: the list, and each command answered with the shutter it moved. */
function server(covers: Shutter[]) {
	return stubFetch((url, init) => {
		if (url === '/api/matter/covers' && (init?.method ?? 'GET') === 'GET') return json({ success: true, covers });
		if (init?.method === 'DELETE') return json({ success: true });
		if (url === '/api/matter/commission') return json({ success: true, cover: shutter({ id: 's9', name: sentBody({ url, init }).name }) });
		const id = url.split('/')[4];
		const c = covers.find((x) => x.id === id) ?? shutter();
		if (url.endsWith('/open')) return json({ success: true, cover: { ...c, motion: 'opening' } });
		if (url.endsWith('/close')) return json({ success: true, cover: { ...c, motion: 'closing' } });
		if (url.endsWith('/stop')) return json({ success: true, cover: { ...c, motion: 'stopped' } });
		if (url.endsWith('/position')) return json({ success: true, cover: { ...c, targetOpenPercent: sentBody({ url, init }).openPercent } });
		if (url === '/api/matter/place') return json({ success: true, place: null });
		return json({ success: true, cover: { ...c, name: sentBody({ url, init }).name } });
	});
}

const commands = (calls: FetchCall[]) =>
	calls.filter((c) => c.init?.method && c.init.method !== 'GET').map((c) => [c.init?.method, c.url, c.init?.body ? sentBody(c) : null]);

describe('Shutters', () => {
	it('says each shutter’s state in words', async () => {
		server([
			shutter({ id: 'a', name: 'A', openPercent: 100 }),
			shutter({ id: 'b', name: 'B', openPercent: 0 }),
			shutter({ id: 'c', name: 'C', openPercent: 40 }),
			shutter({ id: 'd', name: 'D', motion: 'opening' }),
			shutter({ id: 'e', name: 'E', motion: 'closing' }),
			shutter({ id: 'f', name: 'F', openPercent: null }),
			shutter({ id: 'g', name: 'G', online: false })
		]);
		await render(Shutters);
		const tile = (name: string) => page.getByRole('article', { name });
		const state = (name: string) => tile(name).getByRole('paragraph').first();
		await expect.element(state('A')).toHaveTextContent(m.shutters_open());
		await expect.element(state('B')).toHaveTextContent(m.shutters_closed());
		await expect.element(state('C')).toHaveTextContent(m.shutters_open_percent({ percent: 40 }));
		await expect.element(state('D')).toHaveTextContent(m.shutters_opening());
		await expect.element(state('E')).toHaveTextContent(m.shutters_closing());
		await expect.element(state('F')).toHaveTextContent(m.shutters_uncalibrated());
		await expect.element(tile('F').getByText(m.shutters_calibrate_hint())).toBeVisible();
		await expect.element(state('G')).toHaveTextContent(m.state_unreachable());
		await expect.element(tile('G').getByRole('button', { name: m.shutters_open_action() })).toBeDisabled();
		await expect.element(tile('G').getByRole('slider')).not.toBeInTheDocument();
	});

	it('Open, Stop, Close: one POST each to the shutter’s endpoint', async () => {
		const calls = server([shutter()]);
		await render(Shutters);
		await page.getByRole('button', { name: m.shutters_open_action() }).click();
		await expect.element(page.getByText(m.shutters_opening())).toBeVisible();
		await page.getByRole('button', { name: m.common_stop() }).click();
		await expect.poll(() => commands(calls).length).toBe(2);
		await page.getByRole('button', { name: m.shutters_close_action() }).click();
		await expect.poll(() => commands(calls)).toEqual([
			['POST', '/api/matter/covers/s1/open', null],
			['POST', '/api/matter/covers/s1/stop', null],
			['POST', '/api/matter/covers/s1/close', null]
		]);
	});

	it('the position slider says « open at 40 % » and sends the new position on PageUp', async () => {
		const calls = server([shutter()]);
		await render(Shutters);
		const slider = page.getByRole('slider', { name: m.shutters_position() });
		await expect.element(slider).toHaveAttribute('aria-valuetext', m.shutters_open_percent({ percent: 40 }));
		(slider.element() as HTMLElement).focus();
		await userEvent.keyboard('{PageUp}');
		await expect.poll(() => commands(calls)).toEqual([['POST', '/api/matter/covers/s1/position', { openPercent: 65 }]]);
	});

	it('loading, then none: says how to add one', async () => {
		let answer!: (r: Response) => void;
		stubFetch(() => new Promise<Response>((r) => (answer = r)));
		await render(Shutters);
		await expect.element(page.getByText(m.common_loading())).toBeVisible();
		answer(json({ success: true, covers: [] }));
		await expect.element(page.getByText(m.shutters_none())).toBeVisible();
		await expect.element(page.getByText(m.shutters_none_hint())).toBeVisible();
	});

	it('commissions a new shutter with its code and name, then reads the list again', async () => {
		const calls = server([]);
		const toast = vi.spyOn(ui, 'toast');
		await render(Shutters);
		const add = page.getByRole('button', { name: m.common_add() });
		await expect.element(add).toHaveAttribute('aria-expanded', 'false');
		await add.click();
		await expect.element(add).toHaveAttribute('aria-expanded', 'true');
		const pair = page.getByRole('button', { name: m.action_pair() });
		// never disabled without a reason: a missing field is said under it, the focus on it
		await pair.click();
		const code = page.getByLabelText(m.shutters_code());
		await expect.element(code).toHaveAccessibleDescription(m.shutters_code_missing());
		await expect.element(code).toHaveFocus();
		await code.fill('3497-011-2332');
		await page.getByLabelText(m.shutters_name()).fill('Chambre');
		await pair.click();
		await expect.poll(() => toast.mock.calls.length).toBe(1);
		expect(toast).toHaveBeenCalledWith(m.shutters_added({ name: 'Chambre' }));
		expect(commands(calls)).toEqual([['POST', '/api/matter/commission', { code: '3497-011-2332', name: 'Chambre' }]]);
		await expect.element(add).toHaveAttribute('aria-expanded', 'false');
		// the form is gone: the focus is back on « Ajouter »
		await expect.element(add).toHaveFocus();
		await expect.poll(() => calls.filter((c) => c.url === '/api/matter/covers').length).toBe(2);
	});

	it('commissioning: the time elapsed is shown; a refused code is said under the code', async () => {
		let refuse!: () => void;
		stubFetch((url) => {
			if (url === '/api/matter/commission') return new Promise<Response>((r) => (refuse = () => r(json({ success: false, error: 'Code refusé' }, 400))));
			return json({ success: true, covers: [] });
		});
		await render(Shutters);
		await page.getByRole('button', { name: m.common_add() }).click();
		await page.getByLabelText(m.shutters_code()).fill('1111-111-1111');
		await page.getByLabelText(m.shutters_name()).fill('Chambre');
		const pair = page.getByRole('button', { name: m.action_pair() });
		await pair.click();
		await expect.element(page.getByText(m.shutters_commissioning_elapsed({ seconds: 0 }))).toBeVisible();
		await expect.element(pair).toHaveAttribute('aria-busy', 'true');
		refuse();
		const code = page.getByLabelText(m.shutters_code());
		await expect.element(code).toHaveAccessibleDescription('Code refusé');
		await expect.element(code).toHaveFocus();
		await expect.element(code).toHaveValue('1111-111-1111');
	});

	it('renames from its settings with a PATCH; the panel and the focus stay', async () => {
		const calls = server([shutter()]);
		const say = vi.spyOn(ui, 'say');
		await render(Shutters);
		const settings = page.getByRole('button', { name: m.common_settings_of({ name: 'Salon' }) });
		await settings.click();
		await expect.element(settings).toHaveAttribute('aria-expanded', 'true');
		const field = page.getByLabelText(m.shutters_name());
		await field.fill('Séjour');
		await userEvent.keyboard('{Enter}');
		await expect.poll(() => commands(calls)).toEqual([['PATCH', '/api/matter/covers/s1', { name: 'Séjour' }]]);
		await expect.poll(() => say.mock.calls.flat()).toContain(m.common_renamed({ name: 'Séjour' }));
		await expect.element(field).toHaveFocus();
	});

	it('a refused rename keeps the draft and says why under the field', async () => {
		stubFetch((url, init) => {
			if (init?.method === 'PATCH') return json({ success: false, error: 'Nom trop long' }, 400);
			if (url === '/api/matter/place') return json({ success: true, place: null });
			return json({ success: true, covers: [shutter()] });
		});
		await render(Shutters);
		await page.getByRole('button', { name: m.common_settings_of({ name: 'Salon' }) }).click();
		const field = page.getByLabelText(m.shutters_name());
		await field.fill('Séjour');
		await userEvent.keyboard('{Enter}');
		await expect.element(field).toHaveAccessibleDescription('Nom trop long');
		await expect.element(field).toHaveValue('Séjour');
		await expect.element(field).toHaveFocus();
	});

	it('Escape in the name field gives up: the name it had comes back', async () => {
		server([shutter()]);
		await render(Shutters);
		await page.getByRole('button', { name: m.common_settings_of({ name: 'Salon' }) }).click();
		const field = page.getByLabelText(m.shutters_name());
		await field.fill('Séj');
		await userEvent.keyboard('{Escape}');
		await expect.element(field).toHaveValue('Salon');
	});

	it('removes after a confirmation that names it: DELETE, then the list again, the focus on the group', async () => {
		const calls = server([shutter()]);
		const toast = vi.spyOn(ui, 'toast');
		await render(Shutters);
		await page.getByRole('button', { name: m.common_settings_of({ name: 'Salon' }) }).click();
		await page.getByRole('button', { name: m.common_remove_named({ name: 'Salon' }) }).click();
		const dialog = page.getByRole('alertdialog');
		await expect.element(dialog.getByText(m.common_remove_from_maison({ name: 'Salon' }))).toBeVisible();
		await dialog.getByRole('button', { name: m.shutters_remove_action() }).click();
		await expect.poll(() => commands(calls)).toEqual([['DELETE', '/api/matter/covers/s1', null]]);
		await expect.poll(() => toast.mock.calls[0]?.[0]).toBe(m.shutters_removed({ name: 'Salon' }));
	});

	it('a member moves and renames, but neither adds nor removes', async () => {
		session.adopt(alex);
		server([shutter()]);
		await render(Shutters);
		await expect.element(page.getByRole('button', { name: m.shutters_open_action() })).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_add() })).not.toBeInTheDocument();
		await page.getByRole('button', { name: m.common_settings_of({ name: 'Salon' }) }).click();
		await expect.element(page.getByLabelText(m.shutters_name())).toBeVisible();
		await expect.element(page.getByRole('button', { name: m.common_remove_named({ name: 'Salon' }) })).not.toBeInTheDocument();
	});

	it('moving: « Ouvert à 40 % · va à 80 % », the slider’s ends named, a mark where it is', async () => {
		server([shutter({ openPercent: 40, targetOpenPercent: 80, motion: 'opening' })]);
		await render(Shutters);
		await expect.element(page.getByText(`${m.shutters_open_percent({ percent: 40 })} · ${m.shutters_going_to({ percent: 80 })}`)).toBeVisible();
		await expect.element(page.getByText(m.shutters_closed(), { exact: true })).toBeVisible();
		expect(document.querySelector('.range .mark')).not.toBeNull();
	});

	it('a list that cannot be read says so, never « Aucun volet »', async () => {
		stubFetch(() => json({ success: false, error: 'down' }, 500));
		await render(Shutters);
		await expect.element(page.getByText(m.load_failed())).toBeVisible();
		await expect.element(page.getByText(m.shutters_none())).not.toBeInTheDocument();
	});

	it('the tile’s fact is the next scheduled move', async () => {
		const at = '2026-10-02T18:28:00Z';
		server([shutter({ id: 'a', name: 'A', nextClose: at })]);
		await render(Shutters);
		const time = new Intl.DateTimeFormat(document.documentElement.lang || undefined, { hour: 'numeric', minute: '2-digit' }).format(new Date(at));
		await expect.element(page.getByRole('article', { name: 'A' }).getByText(m.shutters_fact_close({ time }))).toBeVisible();
	});

	it('polls every second while a motor runs, every 10 s at rest', async () => {
		vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
		let moving = true;
		const calls = stubFetch(() => json({ success: true, covers: [shutter({ motion: moving ? 'opening' : 'stopped' })] }));
		await render(Shutters);
		// each answer is awaited: a response body settles outside the fake clock
		await expect.poll(() => calls.length).toBe(1);
		await vi.advanceTimersByTimeAsync(1000);
		await expect.poll(() => calls.length).toBe(2);
		moving = false;
		await vi.advanceTimersByTimeAsync(1000);
		await expect.poll(() => calls.length).toBe(3);
		await vi.advanceTimersByTimeAsync(9000);
		expect(calls).toHaveLength(3);
		await vi.advanceTimersByTimeAsync(1000);
		await expect.poll(() => calls.length).toBe(4);
	});
});
