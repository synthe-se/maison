// The AirTies Ruwido remote, row by row: Linux keycodes delivered by the set-top box's KIR
// driver (kernel/bcm7231-kir.c). A key with an icon shows it; one printed with digits or
// letters (« 4 », « OK », « EPG ») shows them, as they are on the plastic, in every language.

import { m } from '#lib/paraglide/messages.js';
import type { IconName } from '#lib/components/Icon.svelte';

export interface RemoteKey {
	code: number;
	/** What the key is called, for the reader and in lists. */
	name: () => string;
	/** What is printed on it, when it is not an icon. */
	glyph?: string;
	icon?: IconName;
}

const printed = (glyph: string): Pick<RemoteKey, 'name' | 'glyph'> => ({ name: () => glyph, glyph });

export const REMOTE_ROWS: RemoteKey[][] = [
	[{ code: 116, name: m.remote_keys_power, icon: 'power' }],
	[
		{ code: 2, ...printed('1') },
		{ code: 3, ...printed('2') },
		{ code: 4, ...printed('3') }
	],
	[
		{ code: 5, ...printed('4') },
		{ code: 6, ...printed('5') },
		{ code: 7, ...printed('6') }
	],
	[
		{ code: 8, ...printed('7') },
		{ code: 9, ...printed('8') },
		{ code: 10, ...printed('9') }
	],
	[
		{ code: 59, ...printed('F1') },
		{ code: 11, ...printed('0') },
		{ code: 60, ...printed('F2') }
	],
	[
		{ code: 14, name: m.remote_keys_erase, icon: 'delete' },
		{ code: 103, name: m.remote_keys_up, icon: 'chevron-up' },
		{ code: 102, name: m.remote_keys_home, icon: 'house' }
	],
	[
		{ code: 105, name: m.remote_keys_left, icon: 'chevron-left' },
		{ code: 353, ...printed('OK') },
		{ code: 106, name: m.remote_keys_right, icon: 'chevron-right' }
	],
	[
		{ code: 388, name: m.remote_keys_text, icon: 'text-align-start' },
		{ code: 108, name: m.remote_keys_down, icon: 'chevron-down' },
		{ code: 358, name: m.remote_keys_info, icon: 'info' }
	],
	[
		{ code: 139, name: m.remote_keys_menu, icon: 'menu' },
		{ code: 365, name: m.remote_keys_guide, glyph: 'EPG' },
		{ code: 226, name: m.remote_keys_zap, glyph: 'ZAP' },
		{ code: 217, name: m.remote_keys_search, icon: 'search' }
	],
	[
		{ code: 115, name: m.remote_keys_volume_up, icon: 'plus' },
		{ code: 167, name: m.remote_keys_record, icon: 'circle' },
		{ code: 402, name: m.remote_keys_channel_up, glyph: 'P+' }
	],
	[
		{ code: 114, name: m.remote_keys_volume_down, icon: 'minus' },
		{ code: 128, name: m.remote_keys_stop, icon: 'square' },
		{ code: 403, name: m.remote_keys_channel_down, glyph: 'P−' }
	],
	[
		{ code: 168, name: m.remote_keys_rewind, icon: 'rewind' },
		{ code: 207, name: m.remote_keys_play, icon: 'play' },
		{ code: 119, name: m.remote_keys_pause, icon: 'pause' },
		{ code: 159, name: m.remote_keys_fast_forward, icon: 'fast-forward' }
	]
];

const BY_CODE = new Map(REMOTE_ROWS.flat().map((k) => [k.code, k]));

/** The key's name (« Volume + », « 4 »), or « n° 412 » for a code not on the picture. */
export function keyName(code: number): string {
	return BY_CODE.get(code)?.name() ?? m.remote_key_number({ code });
}
