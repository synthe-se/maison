// Several photos added at once: one at a time (the Pi holds each while it comes), each resized
// just before it goes, so a hundred photos never sit decoded in the page. One that fails does
// not stop the others: the outcome says which, and what was refused.

import { photosApi } from './api.ts';
import { toJpeg } from './resize.ts';

/** Where the sending is: `done` photos behind it, `sent` (0 to 1) of the one going. */
export interface Sending {
	name: string;
	done: number;
	total: number;
	sent: number;
}

export interface Sent {
	added: number;
	failed: { name: string; error: unknown }[];
}

export async function sendPhotos(files: File[], onStep: (at: Sending) => void): Promise<Sent> {
	const out: Sent = { added: 0, failed: [] };
	const total = files.length;
	for (const [done, file] of files.entries()) {
		const name = file.name;
		onStep({ name, done, total, sent: 0 });
		try {
			await photosApi.add(await toJpeg(file), (sent) => onStep({ name, done, total, sent }));
			out.added++;
		} catch (error) {
			out.failed.push({ name, error });
		}
	}
	return out;
}
