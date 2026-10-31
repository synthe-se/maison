import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import { page, userEvent } from 'vitest/browser';
import { m } from '#lib/paraglide/messages.js';
import { list, percent } from '#lib/i18n.svelte.ts';
import { session } from '#lib/session.svelte.ts';
import { ui } from '#lib/ui.svelte.ts';
import { alex, leonard } from '#lib/test/passkeys.ts';
import { stubApi } from '#lib/test/api.ts';
import PhotosPage from './PhotosPage.svelte';

const photo = (id: string) => ({ id, bytes: 1000 });

/** The backend: `list` at GET /photos, « ok » to a removal; records calls. */
const serve = (list: string[]) => stubApi({ 'GET /photos': { success: true, photos: list.map(photo) } }, { success: true, message: 'ok' });

/** An upload the test drives by hand (`upload` goes through XMLHttpRequest for its progress). */
const xhrs: FakeXhr[] = [];
class FakeXhr {
	upload: { onprogress?: (e: Partial<ProgressEvent>) => void; onload?: () => void } = {};
	onload?: () => void;
	onerror?: () => void;
	status = 0;
	response: unknown = null;
	withCredentials = false;
	responseType = '';
	url = '';
	method = '';
	body: FormData | null = null;
	constructor() {
		xhrs.push(this);
	}
	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}
	send(body: FormData) {
		this.body = body;
	}
	answer(status: number, response: unknown) {
		this.status = status;
		this.response = response;
		this.upload.onload?.();
		this.onload?.();
	}
}

/** A real picture, as a file: a `w` × `h` PNG. */
async function picture(w: number, h: number, name: string): Promise<File> {
	const canvas = document.createElement('canvas');
	canvas.width = w;
	canvas.height = h;
	const context = canvas.getContext('2d')!;
	context.fillStyle = 'red';
	context.fillRect(0, 0, w, h);
	const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/png'));
	return new File([blob!], name, { type: 'image/png' });
}

const picker = () => page.getByLabelText(m.photos_add());
const row = (n: number) => page.getByRole('listitem').nth(n);

beforeEach(() => {
	session.adopt(leonard);
	xhrs.length = 0;
	vi.stubGlobal('XMLHttpRequest', FakeXhr);
});

afterEach(() => {
	ui.toasts = [];
	vi.unstubAllGlobals();
});

describe('PhotosPage', () => {
	it('a member reads why not, and nothing is asked', async () => {
		const api = serve([]);
		session.adopt(alex);
		await render(PhotosPage);
		await expect.element(page.getByRole('heading', { level: 1, name: m.photos_title() })).toBeVisible();
		await expect.element(page.getByText(m.code_forbidden())).toBeVisible();
		await expect.element(picker()).not.toBeInTheDocument();
		expect(api.calls).toEqual([]);
	});

	it('says there are none, and what the frame then does', async () => {
		serve([]);
		await render(PhotosPage);
		await expect.element(page.getByText(m.photos_none())).toBeVisible();
		await expect.element(page.getByText(m.photos_none_hint())).toBeVisible();
	});

	it('shows each photo as a thumbnail, and how many', async () => {
		serve(['a'.repeat(64), 'b'.repeat(64)]);
		await render(PhotosPage);
		await expect.element(page.getByText(m.photos_count({ count: 2 }))).toBeVisible();
		await expect
			.element(page.getByRole('img', { name: m.photos_alt({ number: 2 }) }))
			.toHaveAttribute('src', `/api/photos/${'b'.repeat(64)}`);
	});

	it('removes one only once asked; the focus goes to the list’s title', async () => {
		const id = 'a'.repeat(64);
		const api = serve([id, 'b'.repeat(64)]);
		await render(PhotosPage);
		await row(0)
			.getByRole('button', { name: m.common_remove_named({ name: m.photos_alt({ number: 1 }) }) })
			.click();
		const dialog = page.getByRole('alertdialog', { name: m.photos_remove_title() });
		await dialog.getByRole('button', { name: m.device_keep() }).click();
		expect(api.sent('DELETE', `/photos/${id}`)).toEqual([]);
		await row(0)
			.getByRole('button', { name: m.common_remove_named({ name: m.photos_alt({ number: 1 }) }) })
			.click();
		await dialog.getByRole('button', { name: m.photos_remove_action() }).click();
		await expect.poll(() => api.sent('DELETE', `/photos/${id}`).length).toBe(1);
		await expect.poll(() => api.sent('GET', '/photos').length).toBe(2);
		await expect.element(page.getByRole('heading', { level: 2, name: m.photos_list_title() })).toHaveFocus();
	});

	it('sends what is picked one at a time, resized as a JPEG, with its progress', async () => {
		const api = serve([]);
		const toast = vi.spyOn(ui, 'toast');
		await render(PhotosPage);
		await userEvent.upload(picker(), [await picture(2400, 1800, 'plage.png'), await picture(300, 200, 'chat.png')]);

		await expect.poll(() => xhrs.length).toBe(1);
		const first = xhrs[0];
		expect([first.method, first.url, first.withCredentials]).toEqual(['POST', '/api/photos', true]);
		const sent = first.body!.get('photo') as File;
		expect([sent.name, sent.type]).toEqual(['plage.jpg', 'image/jpeg']);
		const bitmap = await createImageBitmap(sent);
		expect([bitmap.width, bitmap.height]).toEqual([800, 1280]);

		first.upload.onprogress?.({ lengthComputable: true, loaded: 1, total: 2 });
		const bar = page.getByRole('progressbar', { name: m.android_tv_uploading({ name: 'plage.png' }) });
		await expect.element(bar).toHaveAttribute('aria-valuetext', m.photos_progress({ done: 1, total: 2 }));
		await expect.element(bar).toHaveAttribute('aria-valuenow', percent(0.25).replace(/\D/g, ''));
		expect(xhrs).toHaveLength(1);

		first.answer(200, { success: true, photo: photo('a'.repeat(64)) });
		await expect.poll(() => xhrs.length).toBe(2);
		xhrs[1].answer(200, { success: true, photo: photo('b'.repeat(64)) });
		await expect.poll(() => toast.mock.calls.some(([text]) => text === m.photos_added({ count: 2 }))).toBe(true);
		await expect.poll(() => api.sent('GET', '/photos').length).toBe(2);
		await expect.element(picker()).toHaveFocus();
	});

	it('takes what is dropped on it, and marks where it may land', async () => {
		serve([]);
		await render(PhotosPage);
		const zone = page.getByRole('group', { name: m.photos_drop() }).element();
		const files = new DataTransfer();
		files.items.add(await picture(100, 100, 'chat.png'));
		const drag = (type: string) => zone.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer: files }));
		drag('dragover');
		await expect.poll(() => zone.classList.contains('over')).toBe(true);
		drag('dragleave');
		await expect.poll(() => zone.classList.contains('over')).toBe(false);
		drag('drop');
		await expect.poll(() => xhrs.length).toBe(1);
		expect((xhrs[0].body!.get('photo') as File).name).toBe('chat.jpg');
	});

	it('what cannot be read or is refused is said once by reason; the others still go', async () => {
		const api = serve([]);
		const toast = vi.spyOn(ui, 'toast');
		await render(PhotosPage);
		const notes = new File(['hello'], 'notes.txt', { type: 'text/plain' });
		const empty = new File([], 'vide.png', { type: 'image/png' });
		await userEvent.upload(picker(), [notes, await picture(50, 50, 'ok.png'), empty, await picture(60, 60, 'plein.png')]);

		await expect.poll(() => xhrs.length).toBe(1);
		xhrs[0].answer(200, { success: true, photo: photo('a'.repeat(64)) });
		await expect.poll(() => xhrs.length).toBe(2);
		xhrs[1].answer(409, { success: false, error: 'x', code: 'too_many_photos' });

		await expect.poll(() => api.sent('GET', '/photos').length).toBe(2);
		expect(toast).toHaveBeenCalledWith(m.photos_added({ count: 1 }));
		const warned = toast.mock.calls.filter(([, o]) => o?.warn).map(([text]) => text);
		expect(warned).toEqual([
			m.photos_not_added({ names: list(['notes.txt', 'vide.png']), why: m.code_not_a_photo() }),
			m.photos_not_added({ names: 'plein.png', why: m.code_too_many_photos() })
		]);
	});
});
