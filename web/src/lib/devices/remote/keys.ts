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
	[{ code: 116, name: m.key_power, icon: 'power' }],
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
		{ code: 14, name: m.key_erase, icon: 'delete' },
		{ code: 103, name: m.key_up, icon: 'chevron-up' },
		{ code: 102, name: m.nav_home, icon: 'house' }
	],
	[
		{ code: 105, name: m.key_left, icon: 'chevron-left' },
		{ code: 353, ...printed('OK') },
		{ code: 106, name: m.key_right, icon: 'chevron-right' }
	],
	[
		{ code: 388, name: m.key_text, icon: 'text-align-start' },
		{ code: 108, name: m.key_down, icon: 'chevron-down' },
		{ code: 358, name: m.key_info, icon: 'info' }
	],
	[
		{ code: 139, name: m.key_menu, icon: 'menu' },
		{ code: 365, name: m.key_guide, glyph: 'EPG' },
		{ code: 226, name: m.key_zap, glyph: 'ZAP' },
		{ code: 217, name: m.key_search, icon: 'search' }
	],
	[
		{ code: 115, name: m.key_volume_up, icon: 'plus' },
		{ code: 167, name: m.key_record, icon: 'circle' },
		{ code: 402, name: m.key_channel_up, glyph: 'P+' }
	],
	[
		{ code: 114, name: m.key_volume_down, icon: 'minus' },
		{ code: 128, name: m.common_stop, icon: 'square' },
		{ code: 403, name: m.key_channel_down, glyph: 'P−' }
	],
	[
		{ code: 168, name: m.key_rewind, icon: 'rewind' },
		{ code: 207, name: m.key_play, icon: 'play' },
		{ code: 119, name: m.key_pause, icon: 'pause' },
		{ code: 159, name: m.key_fast_forward, icon: 'fast-forward' }
	]
];

const BY_CODE = new Map(REMOTE_ROWS.flat().map((k) => [k.code, k]));

/** The key's name (« Volume + », « 4 »), or « n° 412 » for a code not on the picture. */
export function keyName(code: number): string {
	return BY_CODE.get(code)?.name() ?? m.remote_key_number({ code });
}
