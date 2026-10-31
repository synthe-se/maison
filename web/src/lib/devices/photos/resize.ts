// A photo made ready for the frame, here in the browser: the Pi behind the backend never decodes
// a picture. Scaled to cover the frame's portrait panel, what passes it cut off evenly, never
// enlarged, written as a JPEG: small enough to send and for the frame to decode in a moment, and
// without what the original's metadata says (where it was taken). The browser turns it the way
// its EXIF says (`createImageBitmap`'s default).

import { ApiError } from '#lib/api.ts';
import { PHOTO_SIZE } from '#lib/limits.ts';

const QUALITY = 0.85;

/** What of the picture is kept, and at what size. */
export interface Crop {
	sx: number;
	sy: number;
	sw: number;
	sh: number;
	width: number;
	height: number;
}

/** A `w` × `h` picture over `panel`: covering it at the smallest scale, centred, at most its size
 * (a smaller picture stays as it is: the frame scales it). */
export function cover(w: number, h: number, panel = PHOTO_SIZE): Crop {
	const scale = Math.min(1, Math.max(panel.width / w, panel.height / h));
	const width = Math.min(panel.width, Math.round(w * scale));
	const height = Math.min(panel.height, Math.round(h * scale));
	const [sw, sh] = [width / scale, height / scale];
	return { sx: (w - sw) / 2, sy: (h - sh) / 2, sw, sh, width, height };
}

/** `file` as the frame wants it; what the browser cannot read is refused as the server would. */
export async function toJpeg(file: File): Promise<File> {
	const bitmap = await createImageBitmap(file).catch(() => {
		throw new ApiError('', 415, 'not_a_photo');
	});
	const crop = cover(bitmap.width, bitmap.height);
	const canvas = document.createElement('canvas');
	canvas.width = crop.width;
	canvas.height = crop.height;
	const context = canvas.getContext('2d');
	if (!context) throw new ApiError('', 415, 'not_a_photo');
	context.imageSmoothingQuality = 'high';
	context.drawImage(bitmap, crop.sx, crop.sy, crop.sw, crop.sh, 0, 0, crop.width, crop.height);
	bitmap.close();
	const blob = await new Promise<Blob | null>((done) => canvas.toBlob(done, 'image/jpeg', QUALITY));
	if (!blob) throw new ApiError('', 415, 'not_a_photo');
	return new File([blob], file.name.replace(/(\.[^.]*)?$/, '.jpg'), { type: 'image/jpeg' });
}
