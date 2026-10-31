// The wall frame's photos (backend/src/photos.rs, maison_api::photos): the list, each one's
// bytes, adding one (an upload, for its progress) and removing one.

import { API_BASE, del, get, path, upload } from '#lib/api.ts';

export interface Photo {
	/** The SHA-256 of its bytes. */
	id: string;
	bytes: number;
}

export interface PhotosResponse {
	success: boolean;
	photos: Photo[];
}

/** The field the backend reads the JPEG from (`maison_api::photos::UPLOAD_FIELD`). */
const FIELD = 'photo';
/** A small photo over a house's upload: past this, nothing will come. */
const UPLOAD_TIMEOUT_MS = 2 * 60_000;

export const photosApi = {
	list: () => get<PhotosResponse>('/photos'),
	/** Where a thumbnail reads it: an address, the browser's cookie going with it. */
	src: (id: string) => API_BASE + path`/photos/${id}`,
	add: (file: File, onProgress?: (sent: number) => void) =>
		upload<{ success: boolean; photo: Photo }>('/photos', FIELD, file, UPLOAD_TIMEOUT_MS, onProgress),
	remove: (id: string) => del(path`/photos/${id}`)
};
