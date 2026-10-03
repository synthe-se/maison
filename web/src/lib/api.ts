// The backend's API: JSON in and out with the session cookie, an expired access token renewed
// once and transparently, a refusal thrown with its name (`code`) for the app to word it.

const API_BASE = '/api';

type Method = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

interface ApiOptions {
	method?: Method;
	body?: unknown;
}

/** The server answered, with an error: its message (empty when it gave none: errors.ts words
 * it from the status), the HTTP status, and the refusal's name when it has one
 * (`passkey_rejected`, `last_passkey`…: the app words those itself). A request that got no
 * answer at all (network down) throws the browser's TypeError instead. */
export class ApiError extends Error {
	constructor(
		message: string,
		readonly status: number,
		readonly code?: string,
		/** What the server adds to act on the refusal (e.g. `{person}` for `person_exists`). */
		readonly detail?: Record<string, unknown>
	) {
		super(message);
	}
}

/** The code of a session renewal that got no answer to trust (network down, the tunnel's 502
 * while the Pi restarts): the session may be fine, the server is just not there to say so. */
export const UNREACHABLE = 'unreachable';

/** A name as the backend keeps it: at most this many characters (backend people::MAX_NAME). */
export const MAX_NAME = 60;

// Told when a request stays refused after the renewal: the session is over.
let unauthorized: () => void = () => {};
export function onUnauthorized(handler: () => void) {
	unauthorized = handler;
}

// one renewal at a time: requests refused together wait for the same one
let renewing: Promise<boolean> | null = null;

/**
 * Asks for a new access token. True: renewed; false: the session is over (the refresh token is
 * refused, 401 or 403). No answer, or a 5xx: throws `UNREACHABLE` rather than sign anyone out
 * over a brief outage.
 */
export function renewSession(): Promise<boolean> {
	renewing ??= (async () => {
		let r: Response;
		try {
			r = await fetch(`${API_BASE}/auth/refresh`, { method: 'POST', credentials: 'same-origin' });
		} catch {
			throw new ApiError('', 0, UNREACHABLE);
		}
		if (r.ok) return true;
		if (r.status === 401 || r.status === 403) return false;
		throw new ApiError('', r.status, UNREACHABLE);
	})().finally(() => (renewing = null));
	return renewing;
}

export async function api<T>(endpoint: string, options: ApiOptions = {}): Promise<T> {
	const send = () =>
		fetch(`${API_BASE}${endpoint}`, {
			method: options.method ?? 'GET',
			headers: options.body === undefined ? {} : { 'Content-Type': 'application/json' },
			body: options.body === undefined ? undefined : JSON.stringify(options.body),
			credentials: 'same-origin'
		});

	let response = await send();

	// an expired access token: renewed once, then the request again
	if (response.status === 401) {
		if (await renewSession()) response = await send();
		// the auth routes answer for themselves (a session check, a sign-out)
		if (response.status === 401 && !endpoint.startsWith('/auth/')) unauthorized();
	}

	if (!response.ok) {
		let data: unknown;
		try {
			data = await response.json();
		} catch {
			// not JSON (empty, a proxy's HTML): errors.ts words the status
		}
		throw toApiError(response.status, data);
	}

	// 204 No Content, or an empty body
	const text = await response.text();
	return text ? JSON.parse(text) : ({} as T);
}

/** A refusal as the server said it (`{success:false, error, code?, detail?}`, or nothing to
 * read: a proxy's page, an empty body): the one place an answer becomes an ApiError. */
export function toApiError(status: number, data: unknown): ApiError {
	const d = (data && typeof data === 'object' ? data : {}) as { error?: unknown; code?: unknown; detail?: unknown };
	return new ApiError(
		typeof d.error === 'string' ? d.error : '',
		status,
		typeof d.code === 'string' ? d.code : undefined,
		d.detail && typeof d.detail === 'object' ? (d.detail as Record<string, unknown>) : undefined
	);
}

/** One upload of `file` under `field` (multipart: the browser sets its own boundary).
 * XMLHttpRequest rather than `fetch`: only it reports upload progress. */
function sendFile(endpoint: string, field: string, file: File, timeoutMs: number, onProgress?: (sent: number) => void) {
	return new Promise<{ status: number; data: unknown }>((resolve, reject) => {
		const form = new FormData();
		form.append(field, file);
		const xhr = new XMLHttpRequest();
		xhr.open('POST', `${API_BASE}${endpoint}`);
		xhr.withCredentials = true;
		xhr.responseType = 'json';
		xhr.timeout = timeoutMs;
		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable) onProgress?.(e.loaded / e.total);
		};
		xhr.upload.onload = () => onProgress?.(1);
		// no answer: said as such by errors.ts
		xhr.onerror = xhr.ontimeout = () => reject(new ApiError('', 0, UNREACHABLE));
		xhr.onload = () => resolve({ status: xhr.status, data: xhr.response });
		xhr.send(form);
	});
}

/**
 * Sends a file as `api` sends JSON: `onProgress` gets the fraction sent (0 to 1), an expired
 * access token is renewed once and the file sent again, a refusal thrown with its code.
 */
export async function upload<T>(
	endpoint: string,
	field: string,
	file: File,
	timeoutMs: number,
	onProgress?: (sent: number) => void
): Promise<T> {
	let r = await sendFile(endpoint, field, file, timeoutMs, onProgress);
	if (r.status === 401) {
		if (await renewSession()) r = await sendFile(endpoint, field, file, timeoutMs, onProgress);
		if (r.status === 401) unauthorized();
	}
	if (r.status < 200 || r.status >= 300) throw toApiError(r.status, r.data);
	return (r.data ?? {}) as T;
}

/** A path with each interpolated value encoded (an id never breaks out of its segment):
 * path`/meross/${id}/status`. */
export function path(strings: TemplateStringsArray, ...values: (string | number)[]): string {
	return strings.reduce((out, s, i) => out + s + (i < values.length ? encodeURIComponent(String(values[i])) : ''), '');
}

export const get = <T>(endpoint: string) => api<T>(endpoint);
export const post = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: 'POST', body });
export const put = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: 'PUT', body });
export const patch = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: 'PATCH', body });
export const del = <T = SimpleResponse>(endpoint: string) => api<T>(endpoint, { method: 'DELETE' });

/** What a command answers when it has nothing else to say (backend routes::SimpleResponse). */
export interface SimpleResponse {
	success: boolean;
	message: string;
}

/** A device as a command's answer names it. */
export interface DeviceRef {
	id: string;
	name: string;
}
