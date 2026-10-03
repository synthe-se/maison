// The Philips TV (JointSPACE, IR for power: backend/src/tv.rs) and the Android TV box
// (Remote v2 and ADB: backend/src/androidtv.rs).

import { get, post, put, upload, type SimpleResponse } from '#lib/api.ts';
export type TvPower = 'on' | 'standby' | 'deep_standby';

export type TvKey =
	| 'standby'
	| 'back'
	| 'home'
	| 'source'
	| 'watch_tv'
	| 'confirm'
	| 'cursor_up'
	| 'cursor_down'
	| 'cursor_left'
	| 'cursor_right'
	| 'volume_up'
	| 'volume_down'
	| 'mute'
	| 'channel_step_up'
	| 'channel_step_down'
	| 'play_pause'
	| 'pause'
	| 'stop'
	| 'fast_forward'
	| 'rewind'
	| 'next'
	| 'previous'
	| 'info'
	| 'options'
	| 'subtitle'
	| 'teletext'
	| 'ambilight_on_off';

export interface TvConfig {
	host?: string | null;
	irBlasterHost?: string | null;
	boxHost?: string | null;
	boxWakeApp?: string | null;
}

export interface TvVolume {
	current: number;
	min: number;
	max: number;
	muted: boolean;
}

export interface TvAmbilight {
	power: boolean;
	mode?: string;
	style?: string;
	setting?: string;
}

export interface TvStatus {
	configured: boolean;
	power: TvPower;
	name?: string;
	volume?: TvVolume;
	ambilight?: TvAmbilight;
}

export interface TvStatusResponse {
	success: boolean;
	config: TvConfig;
	status: TvStatus;
}

export const tvApi = {
	status: () => get<TvStatusResponse>('/tv'),
	setConfig: (config: TvConfig) => put('/tv/config', config),
	power: (state: 'on' | 'off' | 'toggle', switchToBox = true) =>
		post<{ success: boolean; power: TvPower }>('/tv/power', { state, switchToBox }),
	sendKey: (key: TvKey) => post('/tv/key', { key }),
	setVolume: (level?: number, muted?: boolean) => put<{ success: boolean; volume: TvVolume }>('/tv/volume', { level, muted }),
	ambilight: (state: 'on' | 'off' | 'toggle') => post<{ success: boolean; ambilight: TvAmbilight }>('/tv/ambilight', { state }),
	switchToBox: () => post('/tv/source/box')
};

export type AndroidKey =
	| 'up'
	| 'down'
	| 'left'
	| 'right'
	| 'ok'
	| 'back'
	| 'home'
	| 'menu'
	| 'search'
	| 'volume_up'
	| 'volume_down'
	| 'mute'
	| 'play_pause'
	| 'play'
	| 'pause'
	| 'stop'
	| 'next'
	| 'previous'
	| 'rewind'
	| 'fast_forward'
	| 'channel_up'
	| 'channel_down'
	| 'power'
	| 'sleep'
	| 'wakeup';

export interface AndroidApp {
	package: string;
	label: string;
}

export interface AndroidTvConfig {
	host?: string | null;
	port?: number | null;
	favoriteApps?: AndroidApp[];
}

export interface AndroidTvStatus {
	configured: boolean;
	reachable: boolean;
	awake: boolean;
	currentApp?: string;
	model?: string;
	/** True once paired over Remote v2, which is what makes keys fast. */
	paired: boolean;
}

export interface AndroidTvStatusResponse {
	success: boolean;
	config: AndroidTvConfig;
	status: AndroidTvStatus;
}

/** An APK upload and its install answer together within this long (tens of MB over the Pi's
 * link, then `pm install`). */
const APK_TIMEOUT_MS = 15 * 60_000;

export const androidTvApi = {
	status: () => get<AndroidTvStatusResponse>('/androidtv'),
	setConfig: (config: AndroidTvConfig) => put('/androidtv/config', config),
	sendKey: (key: AndroidKey) => post('/androidtv/key', { key }),
	launch: (pkg: string, ensureTvOn = true) => post('/androidtv/launch', { package: pkg, ensureTvOn }),
	apps: () => get<{ success: boolean; packages: string[] }>('/androidtv/apps'),
	wake: () => post('/androidtv/wake'),
	sleep: () => post('/androidtv/sleep'),
	pairStart: () => post('/androidtv/pair/start'),
	pairFinish: (code: string) => post('/androidtv/pair/finish', { code }),

	/**
	 * Sideloads an APK onto the box: an APK is tens of MB on a Pi's link, so the upload says how
	 * far it got (`onProgress`, 0 to 1); at 1 the box is installing, and the answer comes when
	 * `pm install` is done.
	 */
	installApk: (file: File, onProgress?: (sent: number) => void) =>
		upload<SimpleResponse>('/androidtv/apk', 'apk', file, APK_TIMEOUT_MS, onProgress)
};
