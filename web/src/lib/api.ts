// The backend's API: JSON in and out with the session cookie, an expired access token renewed
// once and transparently, a refusal thrown with its name (`code`) for the app to word it.

const API_BASE = "/api";

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

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
		readonly detail?: Record<string, unknown>,
	) {
		super(message);
	}
}

/** The code of a session renewal that got no answer to trust (network down, the tunnel's 502
 * while the Pi restarts): the session may be fine, the server is just not there to say so. */
export const UNREACHABLE = "unreachable";

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
			r = await fetch(`${API_BASE}/auth/refresh`, { method: "POST", credentials: "same-origin" });
		} catch {
			throw new ApiError("", 0, UNREACHABLE);
		}
		if (r.ok) return true;
		if (r.status === 401 || r.status === 403) return false;
		throw new ApiError("", r.status, UNREACHABLE);
	})().finally(() => (renewing = null));
	return renewing;
}

export async function api<T>(endpoint: string, options: ApiOptions = {}): Promise<T> {
	const send = () =>
		fetch(`${API_BASE}${endpoint}`, {
			method: options.method ?? "GET",
			headers: options.body === undefined ? {} : { "Content-Type": "application/json" },
			body: options.body === undefined ? undefined : JSON.stringify(options.body),
			credentials: "same-origin",
		});

	let response = await send();

	// an expired access token: renewed once, then the request again
	if (response.status === 401) {
		if (await renewSession()) response = await send();
		// the auth routes answer for themselves (a session check, a sign-out)
		if (response.status === 401 && !endpoint.startsWith("/auth/")) unauthorized();
	}

	if (!response.ok) {
		let message = "";
		let code: string | undefined;
		let detail: Record<string, unknown> | undefined;
		try {
			const data = await response.json();
			if (data.error) message = data.error;
			code = data.code;
			detail = data.detail;
		} catch {
			// not JSON (empty, a proxy's HTML): errors.ts words the status
		}
		throw new ApiError(message, response.status, code, detail);
	}

	// 204 No Content, or an empty body
	const text = await response.text();
	return text ? JSON.parse(text) : ({} as T);
}

/** A path with each interpolated value encoded (an id never breaks out of its segment):
 * path`/meross/${id}/status`. */
export function path(strings: TemplateStringsArray, ...values: (string | number)[]): string {
	return strings.reduce((out, s, i) => out + s + (i < values.length ? encodeURIComponent(String(values[i])) : ""), "");
}

export const get = <T>(endpoint: string) => api<T>(endpoint);
export const post = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: "POST", body });
export const put = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: "PUT", body });
export const patch = <T = SimpleResponse>(endpoint: string, body?: unknown) => api<T>(endpoint, { method: "PATCH", body });
export const del = <T = SimpleResponse>(endpoint: string) => api<T>(endpoint, { method: "DELETE" });

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

// ── the session: signing in is passkeys.ts ──

/** Who is signed in. `role`: "admin" (may invite and configure) or "member". */
export interface User {
	id: string;
	name: string;
	role: string;
}

export interface SessionResponse {
	success: boolean;
	user?: User;
	error?: string;
}

export const authApi = {
	verify: () => post<SessionResponse>("/auth/verify"),
	logout: () => post("/auth/logout"),
	/** Every session of mine ends, this one too. */
	logoutEverywhere: () => post("/auth/logout-everywhere"),
};

/** Someone who may come in (admin only): their passkeys counted. */
export interface Person {
	id: string;
	name: string;
	role: string;
	passkeys: number;
}

export const peopleApi = {
	list: () => get<Person[]>("/people"),
	/** The person, their passkeys and sessions go (never oneself). */
	remove: (id: string) => del<void>(path`/people/${id}`),
};

// ── Tuya devices (the cats' corner) ──

export interface Device {
	id: string;
	name: string;
	type: "feeder" | "litter-box" | "fountain" | "unknown";
	product_name?: string;
	model?: string;
	ip?: string;
	version?: string;
	connected: boolean;
	last_data?: unknown;
	parsed_data?: unknown;
}

export interface DevicesResponse {
	success: boolean;
	devices: Device[];
	total: number;
	message: string;
}

export interface DeviceStatusResponse<T = unknown> {
	success: boolean;
	device: DeviceRef & { type: string; connected: boolean };
	parsed_status: T;
	raw_dps?: unknown;
	message: string;
}

/** What the backend parses from the feeder (backend tuya.rs), plus what older firmwares said. */
export interface FeederStatus {
	food_level?: string;
	battery_level?: number;
	is_feeding?: boolean;
	error?: string;
	system?: { fault_status?: boolean; powered_by?: string };
}

export interface FountainStatus {
	power?: boolean;
	/** Older firmwares said `uv_enabled`, the backend says `uv`. */
	uv?: boolean;
	uv_enabled?: boolean;
	/** Seconds of UV left; > 0 means the lamp is on. */
	uv_runtime?: number;
	/** 1 or 2. */
	eco_mode?: number;
	water_level?: string;
	/** Minutes. */
	filter_life?: number;
	pump_time?: number;
	water_time?: number;
}

export interface LitterBoxSettings {
	clean_delay?: number;
	sleep_mode?: { enabled?: boolean; start_time?: string; end_time?: string };
	preferences?: {
		child_lock?: boolean;
		kitten_mode?: boolean;
		lighting?: boolean;
		prompt_sound?: boolean;
		automatic_homing?: boolean;
	};
	actions?: { reset_sand_level?: boolean; reset_factory_settings?: boolean };
}

export type LitterBoxPreference = keyof NonNullable<LitterBoxSettings["preferences"]>;

export interface LitterBoxStatus {
	clean_delay?: { seconds?: number };
	sleep_mode?: { enabled?: boolean; start_time_formatted?: string; end_time_formatted?: string };
	sensors?: { litter_level?: string; fault_alarm?: number };
	system?: { state?: string; maintenance_required?: boolean };
	settings?: Partial<Record<LitterBoxPreference, boolean>>;
}

export interface MealPlanEntry {
	days_of_week: string[];
	time: string;
	portion: number;
	status: "Enabled" | "Disabled";
}

export interface MealPlanResponse {
	success: boolean;
	device: DeviceRef;
	decoded: MealPlanEntry[] | null;
	meal_plan: string | null;
	message: string;
}

export const devicesApi = {
	list: () => get<DevicesResponse>("/devices"),
	connect: (id: string) => post(path`/devices/${id}/connect`),
	connectAll: () => post("/devices/connect"),
	disconnect: (id: string) => post(path`/devices/${id}/disconnect`),
	disconnectAll: () => post("/devices/disconnect"),
};

export const feederApi = {
	status: (id: string) => get<DeviceStatusResponse<FeederStatus>>(path`/devices/${id}/feeder/status`),
	feed: (id: string, portion = 1) => post(path`/devices/${id}/feeder/feed`, { portion }),
	getMealPlan: (id: string) => get<MealPlanResponse>(path`/devices/${id}/feeder/meal-plan`),
	setMealPlan: (id: string, mealPlan: MealPlanEntry[]) => post(path`/devices/${id}/feeder/meal-plan`, { meal_plan: mealPlan }),
};

export const fountainApi = {
	status: (id: string) => get<DeviceStatusResponse<FountainStatus>>(path`/devices/${id}/fountain/status`),
	power: (id: string, enabled: boolean) => post(path`/devices/${id}/fountain/power`, { enabled }),
	resetWater: (id: string) => post(path`/devices/${id}/fountain/reset/water`),
	resetFilter: (id: string) => post(path`/devices/${id}/fountain/reset/filter`),
	resetPump: (id: string) => post(path`/devices/${id}/fountain/reset/pump`),
	setUV: (id: string, enabled: boolean) => post(path`/devices/${id}/fountain/uv`, { enabled }),
	/** 1 = mode 1, 2 = mode 2. */
	setEcoMode: (id: string, mode: number) => post(path`/devices/${id}/fountain/eco-mode`, { mode }),
};

export const litterBoxApi = {
	status: (id: string) => get<DeviceStatusResponse<LitterBoxStatus>>(path`/devices/${id}/litter-box/status`),
	clean: (id: string) => post(path`/devices/${id}/litter-box/clean`),
	settings: (id: string, settings: LitterBoxSettings) => post(path`/devices/${id}/litter-box/settings`, settings),
};

// ── Hue lamps (Bluetooth) ──

export interface HueLampState {
	isOn: boolean;
	brightness: number;
	temperature: number | null;
	temperatureMin: number | null;
	temperatureMax: number | null;
}

export interface HueLamp {
	id: string;
	name: string;
	address: string;
	model: string | null;
	manufacturer: string;
	firmware: string | null;
	connected: boolean;
	connecting: boolean;
	reachable: boolean;
	state: HueLampState;
	lastSeen: string | null;
}

export interface HueLampsResponse {
	success: boolean;
	lamps: HueLamp[];
	total: number;
	connected: number;
	reachable: number;
	message: string;
}

export interface HueLampStatusResponse {
	success: boolean;
	lamp?: HueLamp;
	message?: string;
	error?: string;
}

export interface HueLampActionResponse {
	success: boolean;
	state?: { isOn: boolean; brightness: number };
	message?: string;
	error?: string;
}

/** A lamp family's counts; `disabled` when the server runs without that radio. */
export interface LampStats {
	success: boolean;
	total: number;
	connected: number;
	reachable: number;
	disabled?: boolean;
	message?: string;
}

export const hueLampsApi = {
	list: () => get<HueLampsResponse>("/hue-lamps"),
	scan: () => post("/hue-lamps/scan"),
	stats: () => get<LampStats>("/hue-lamps/stats"),
	status: (id: string) => get<HueLampStatusResponse>(path`/hue-lamps/${id}`),
	power: (id: string, enabled: boolean) => post<HueLampActionResponse>(path`/hue-lamps/${id}/power`, { enabled }),
	brightness: (id: string, brightness: number) => post<HueLampActionResponse>(path`/hue-lamps/${id}/brightness`, { brightness }),
	temperature: (id: string, temperature: number) => post<HueLampActionResponse>(path`/hue-lamps/${id}/temperature`, { temperature }),
	blacklist: (id: string) => post(path`/hue-lamps/${id}/blacklist`),
};

// ── Zigbee lamps (the server's own coordinator) ──

export interface ZigbeeLampState {
	isOn: boolean;
	brightness: number;
	temperature: number | null;
	temperatureMin: number | null;
	temperatureMax: number | null;
	colorX: number | null;
	colorY: number | null;
	colorMode: number | null;
}

export interface ZigbeeLamp {
	id: string;
	name: string;
	address: string;
	friendlyName: string;
	interviewCompleted: boolean;
	model: string | null;
	manufacturer: string;
	connected: boolean;
	reachable: boolean;
	supportsBrightness: boolean;
	supportsTemperature: boolean;
	supportsColor: boolean;
	state: ZigbeeLampState;
	lastSeen: string | null;
}

export interface ZigbeeLampsResponse {
	success: boolean;
	lamps: ZigbeeLamp[];
	total: number;
	connected: number;
	reachable: number;
	message: string;
}

export interface ZigbeeLampStatusResponse {
	success: boolean;
	lamp?: ZigbeeLamp;
	message?: string;
	error?: string;
}

export interface ZigbeePairingStatus {
	active: boolean;
	remainingSeconds: number;
	permitJoinSeconds: number;
	message?: string;
}

export interface ZigbeePairingResponse {
	success: boolean;
	pairing: ZigbeePairingStatus;
	message: string;
}

export interface ZigbeeLampActionResponse {
	success: boolean;
	state?: ZigbeeLampState;
	message?: string;
	error?: string;
}

export const zigbeeLampsApi = {
	list: () => get<ZigbeeLampsResponse>("/zigbee/lamps"),
	stats: () => get<LampStats>("/zigbee/lamps/stats"),
	status: (id: string) => get<ZigbeeLampStatusResponse>(path`/zigbee/lamps/${id}`),
	pairingStatus: () => get<ZigbeePairingResponse>("/zigbee/lamps/pairing/status"),
	startPairing: () => post<ZigbeePairingResponse>("/zigbee/lamps/pairing/start"),
	stopPairing: () => post<ZigbeePairingResponse>("/zigbee/lamps/pairing/stop"),
	touchlinkScan: () => post("/zigbee/lamps/pairing/touchlink"),
	power: (id: string, enabled: boolean) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/power`, { enabled }),
	brightness: (id: string, brightness: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/brightness`, { brightness }),
	temperature: (id: string, temperature: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/temperature`, { temperature }),
	color: (id: string, x: number, y: number) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/color`, { x, y }),
	effect: (id: string, effect: string) => post<ZigbeeLampActionResponse>(path`/zigbee/lamps/${id}/effect`, { effect }),
	rename: (id: string, name: string) => post(path`/zigbee/lamps/${id}/rename`, { name }),
};

// ── Meross plugs ──

export interface MerossPlug {
	id: string;
	name: string;
	ip: string;
	isOnline: boolean;
	isOn: boolean;
	/** When the plug last answered (ms since the epoch); 0: never. */
	lastPing: number;
}

export interface MerossPlugsResponse {
	success: boolean;
	devices: MerossPlug[];
	total: number;
	message: string;
}

export interface MerossPlugStatus {
	online: boolean;
	on: boolean;
	electricity: { voltage: number; current: number; power: number } | null;
	hardware: { type: string; version: string; chipType: string; uuid: string; mac: string } | null;
	firmware: { version: string; compileTime: string; innerIp: string } | null;
	wifi: { signal: number | null };
	lastUpdate: number;
}

export interface MerossPlugStatusResponse {
	success: boolean;
	device: DeviceRef;
	status: MerossPlugStatus;
	message: string;
}

export interface MerossElectricityResponse {
	success: boolean;
	device: DeviceRef;
	electricity: {
		voltage: string;
		current: string;
		power: string;
		raw: {
			channel: number;
			current: number;
			voltage: number;
			power: number;
			config?: { voltageRatio: number; electricityRatio: number };
		};
	};
	message: string;
}

export interface MerossToggleResponse {
	success: boolean;
	device: DeviceRef;
	on: boolean;
	message: string;
}

export interface MerossConsumptionResponse {
	success: boolean;
	device: DeviceRef;
	consumption: Array<{ date: string; time: number; value: number }>;
	summary: { days: number; totalWh: number; totalKwh: number };
	message: string;
}

export const merossApi = {
	list: () => get<MerossPlugsResponse>("/meross"),
	status: (id: string) => get<MerossPlugStatusResponse>(path`/meross/${id}/status`),
	electricity: (id: string) => get<MerossElectricityResponse>(path`/meross/${id}/electricity`),
	toggle: (id: string, on: boolean) => post<MerossToggleResponse>(path`/meross/${id}/toggle`, { on }),
	consumption: (id: string) => get<MerossConsumptionResponse>(path`/meross/${id}/consumption`),
	dnd: (id: string, enabled: boolean) => post(path`/meross/${id}/dnd`, { enabled }),
};

// ── Broadlink (the IR blaster) and the Mitsubishi air conditioner ──

export interface BroadlinkDevice {
	host: string;
	mac: string;
	modelCode: number;
	friendlyModel: string;
	friendlyType: string;
	name: string;
	isLocked: boolean;
	kind: string;
	supportsLearning: boolean;
}

export interface BroadlinkCode {
	id: string;
	name: string;
	brand: string | null;
	model: string | null;
	command: string;
	packetBase64: string;
	packetLength: number;
	tags: string[];
	createdAt: string;
	updatedAt: string;
}

export interface BroadlinkDiscoverResponse {
	success: boolean;
	devices: BroadlinkDevice[];
	total: number;
	message: string;
}

export interface BroadlinkCodesResponse {
	success: boolean;
	codes: BroadlinkCode[];
	total: number;
	message: string;
}

export interface BroadlinkSendResponse {
	success: boolean;
	result: { host: string; codeId?: string; command?: string; packetLength: number };
	message: string;
}

export interface BroadlinkClimateSettings {
	mode: string;
	temperature: number;
	fan: string;
	vane: string;
	econo: boolean;
	stopInMinutes: number | null;
}

export interface BroadlinkClimateState {
	power: boolean;
	lastCommand: string;
	lastOnCommand: string | null;
	/** Parsed form of lastOnCommand, provided by the backend. */
	settings: BroadlinkClimateSettings | null;
	host: string;
	model: string | null;
	updatedAt: string;
}

export interface BroadlinkClimateStateResponse {
	success: boolean;
	state: BroadlinkClimateState | null;
	message: string;
}

export const broadlinkApi = {
	discover: (localIp?: string, forceRefresh = false) => {
		const query = new URLSearchParams();
		if (localIp) query.set("localIp", localIp);
		if (forceRefresh) query.set("forceRefresh", "true");
		const q = query.toString();
		return get<BroadlinkDiscoverResponse>(`/broadlink/discover${q ? `?${q}` : ""}`);
	},
	listCodes: () => get<BroadlinkCodesResponse>("/broadlink/codes"),
	getMitsubishiState: () => get<BroadlinkClimateStateResponse>("/broadlink/mitsubishi/state"),
	sendMitsubishiCommand: (host: string, command: string, model?: string, localIp?: string) =>
		post<BroadlinkSendResponse>("/broadlink/mitsubishi/send", { host, command, model, localIp }),
};

// ── Tempo (RTE colours, the forecasts) ──

export type TempoColor = "BLUE" | "WHITE" | "RED";
export type TempoProbabilities = Record<TempoColor, number>;

/** €/kWh, tax included: off-peak and peak. */
export interface TempoPrice {
	hc: number;
	hp: number;
}

export interface TempoTarifs {
	blue: TempoPrice;
	white: TempoPrice;
	red: TempoPrice;
	/** The yearly subscription (6 kVA), €. */
	subscription: number | null;
	dateDebut: string;
}

export interface TempoCount {
	used: number;
	total: number;
	remaining: number;
}

/** A season's days per colour: published (tomorrow included), quota, left. */
export interface TempoStock {
	season: string;
	blue: TempoCount;
	white: TempoCount;
	red: TempoCount;
}

export interface TempoDay {
	date: string;
	/** Null until RTE publishes it. */
	color: TempoColor | null;
}

/** Peak hours (« HH:MM », local): off-peak the rest of the day. */
export interface TempoHours {
	peak_start: string;
	peak_end: string;
}

export interface TempoToday {
	success: boolean;
	/** In force until 06:00 (a Tempo day runs 06:00 to 06:00). */
	yesterday: TempoDay;
	today: TempoDay;
	tomorrow: TempoDay;
	tarifs: TempoTarifs | null;
	hours: TempoHours;
	stock: TempoStock;
	lastUpdated: string | null;
	/** The sources did not answer: what is shown is older. */
	cached: boolean;
}

export interface TempoScore {
	horizon: number;
	days: number;
	accuracy: number;
	winter_accuracy: number;
	red_f1: number;
	white_f1: number;
	brier: number;
	always_blue: number;
}

export interface TempoForecastDay {
	date: string;
	/** Days after today. */
	horizon: number;
	/** RTE's colour, not a forecast. */
	official: boolean;
	color: TempoColor;
	probabilities: TempoProbabilities;
	confidence: number;
	reliability?: { accuracy: number; winter_accuracy: number };
}

export interface TempoForecast {
	success: boolean;
	issued: string;
	/** The weather forecast's day; older than today when Open-Meteo did not answer. */
	weather_issued: string | null;
	stale: boolean;
	model: { version: string; fitted_through: string; backtest: { seasons: string[]; horizons: TempoScore[] } } | null;
	days: TempoForecastDay[];
	stock: TempoStock;
	note?: string;
}

export interface TempoCalendarDay {
	date: string;
	color: TempoColor;
	is_actual: boolean;
	is_prediction: boolean;
	probabilities?: TempoProbabilities;
	confidence?: number;
}

export interface TempoCalendar {
	success: boolean;
	season: string;
	calendar: TempoCalendarDay[];
	stock: TempoStock;
}

export const tempoApi = {
	get: () => get<TempoToday>("/tempo"),
	forecast: () => get<TempoForecast>("/tempo/forecast"),
	calendar: (season?: string) => get<TempoCalendar>(season ? path`/tempo/calendar?season=${season}` : "/tempo/calendar"),
};

// ── Nabaztag (garenne) ──

export interface NabaztagConfig {
	host: string | null;
	tempoEnabled: boolean;
}

export interface NabaztagStatusResponse {
	success: boolean;
	config: NabaztagConfig;
	reachable: boolean;
}

export interface NabaztagTempoPushResponse {
	success: boolean;
	message: string;
	result: { todayColor: string; tomorrowColor: string | null; ledHex: string; earPosition: number | null };
}

export const nabaztagApi = {
	status: () => get<NabaztagStatusResponse>("/nabaztag"),
	pushTempo: (forceRefresh = false) => post<NabaztagTempoPushResponse>("/nabaztag/tempo/push", { forceRefresh }),
};

// ── IR remote (AirTies STB) ──

/** Force a state, or flip the current one — the remote-control default. */
export type IrSwitchState = "on" | "off" | "toggle";

export type IrAction =
	| { action: "nabaztag"; command: string }
	| { action: "zigbee_power"; lamp: string; state: IrSwitchState }
	| { action: "zigbee_brightness"; lamp: string; brightness: number }
	| { action: "broadlink_code"; host: string; code_id: string }
	| { action: "meross_power"; device: string; state: IrSwitchState }
	| {
			action: "climate_toggle";
			host: string;
			/** Structured Mitsubishi command, e.g. "state-cool-16-fan-4-vane-swing". */
			on_command: string;
			model?: string;
	  };

export interface IrBinding {
	/** Fired in order; one failing action does not stop the others. */
	actions: IrAction[];
	label?: string;
	/** Also fire on kernel autorepeat events while the button is held. */
	repeat?: boolean;
}

export interface IrKeymapResponse {
	success: boolean;
	/** Keycodes are numbers, serialized as JSON object keys (strings). */
	keymap: Record<string, IrBinding>;
}

export interface IrEvent {
	/** One more with each event since the backend started: what capture compares. */
	seq: number;
	code: number;
	/** 1 = press, 2 = autorepeat, 0 = release. */
	value: number;
	mapped: boolean;
	receivedAt: string;
}

export interface IrRecentResponse {
	success: boolean;
	/** Newest first. */
	events: IrEvent[];
}

export interface IrTestResponse {
	success: boolean;
	message: string;
	/** One entry per executed action ("ok: ..." / "failed: ..."). */
	results: string[];
}

export const irApi = {
	keymap: () => get<IrKeymapResponse>("/ir/keymap"),
	setBinding: (code: number, binding: IrBinding) => put(path`/ir/keymap/${code}`, binding),
	removeBinding: (code: number) => del(path`/ir/keymap/${code}`),
	recent: () => get<IrRecentResponse>("/ir/recent"),
	test: (actions: IrAction[]) => post<IrTestResponse>("/ir/test", { actions }),
};

// ── the Philips TV (JointSPACE, IR for power) ──

export type TvPower = "on" | "standby" | "deep_standby";

export type TvKey =
	| "standby"
	| "back"
	| "home"
	| "source"
	| "watch_tv"
	| "confirm"
	| "cursor_up"
	| "cursor_down"
	| "cursor_left"
	| "cursor_right"
	| "volume_up"
	| "volume_down"
	| "mute"
	| "channel_step_up"
	| "channel_step_down"
	| "play_pause"
	| "pause"
	| "stop"
	| "fast_forward"
	| "rewind"
	| "next"
	| "previous"
	| "info"
	| "options"
	| "subtitle"
	| "teletext"
	| "ambilight_on_off";

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
	status: () => get<TvStatusResponse>("/tv"),
	setConfig: (config: TvConfig) => put("/tv/config", config),
	power: (state: "on" | "off" | "toggle", switchToBox = true) =>
		post<{ success: boolean; power: TvPower }>("/tv/power", { state, switchToBox }),
	sendKey: (key: TvKey) => post("/tv/key", { key }),
	setVolume: (level?: number, muted?: boolean) => put<{ success: boolean; volume: TvVolume }>("/tv/volume", { level, muted }),
	ambilight: (state: "on" | "off" | "toggle") => post<{ success: boolean; ambilight: TvAmbilight }>("/tv/ambilight", { state }),
	switchToBox: () => post("/tv/source/box"),
};

// ── Matter window coverings (Sonoff Orb-RBS) ──
// Positions are "how open": 100 = fully open, 0 = closed; null until the switch is calibrated.

export type ShutterMotion = "stopped" | "opening" | "closing";

export interface Shutter {
	id: string;
	name: string;
	endpoint: number;
	online: boolean;
	openPercent: number | null;
	targetOpenPercent: number | null;
	motion: ShutterMotion | null;
	vendorId: number | null;
	productId: number | null;
	schedule: SunSchedule;
	/** When the sun schedule will next open / close it (ISO, UTC). */
	nextOpen: string | null;
	nextClose: string | null;
	error?: string;
}

/** Follow the sun: open at sunrise, close at sunset, each shifted by minutes (± 180). */
export interface SunSchedule {
	openAtSunrise: boolean;
	closeAtSunset: boolean;
	sunriseOffsetMin: number;
	sunsetOffsetMin: number;
}

/** Where the house is (for the sun schedule). */
export interface Place {
	name: string;
	latitude: number;
	longitude: number;
}

export interface ShuttersResponse {
	success: boolean;
	covers: Shutter[];
}

export interface ShutterResponse {
	success: boolean;
	cover: Shutter;
}

export const shuttersApi = {
	list: () => get<ShuttersResponse>("/matter/covers"),
	commission: (code: string, name: string) => post<ShutterResponse>("/matter/commission", { code, name }),
	open: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/open`),
	close: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/close`),
	stop: (id: string) => post<ShutterResponse>(path`/matter/covers/${id}/stop`),
	setPosition: (id: string, openPercent: number) => post<ShutterResponse>(path`/matter/covers/${id}/position`, { openPercent }),
	rename: (id: string, name: string) => patch<ShutterResponse>(path`/matter/covers/${id}`, { name }),
	remove: (id: string) => del<{ success: boolean }>(path`/matter/covers/${id}`),
	setSchedule: (id: string, schedule: SunSchedule) => put<ShutterResponse>(path`/matter/covers/${id}/schedule`, schedule),
	place: () => get<{ success: boolean; place: Place | null }>("/matter/place"),
	setPlace: (place: Place) => put<{ success: boolean; place: Place }>("/matter/place", place),
	searchPlaces: (query: string, lang: string) =>
		get<{ success: boolean; places: Place[] }>(`/matter/place/search?${new URLSearchParams({ q: query, lang })}`),
};

// ── the Android TV box ──

export type AndroidKey =
	| "up"
	| "down"
	| "left"
	| "right"
	| "ok"
	| "back"
	| "home"
	| "menu"
	| "search"
	| "volume_up"
	| "volume_down"
	| "mute"
	| "play_pause"
	| "play"
	| "pause"
	| "stop"
	| "next"
	| "previous"
	| "rewind"
	| "fast_forward"
	| "channel_up"
	| "channel_down"
	| "power"
	| "sleep"
	| "wakeup";

export interface AndroidApp {
	package: string;
	label: string;
}

export interface AndroidTvConfig {
	host?: string | null;
	port?: number | null;
	favouriteApps?: AndroidApp[];
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
export const APK_TIMEOUT_MS = 15 * 60_000;

/** One upload of `file` (multipart: the browser sets its own boundary). */
function upload(file: File, onProgress?: (sent: number) => void): Promise<{ status: number; data: { error?: string } & SimpleResponse }> {
	return new Promise((resolve, reject) => {
		const form = new FormData();
		form.append("apk", file);
		const xhr = new XMLHttpRequest();
		xhr.open("POST", `${API_BASE}/androidtv/apk`);
		xhr.withCredentials = true;
		xhr.responseType = "json";
		xhr.timeout = APK_TIMEOUT_MS;
		xhr.upload.onprogress = (e) => {
			if (e.lengthComputable) onProgress?.(e.loaded / e.total);
		};
		xhr.upload.onload = () => onProgress?.(1);
		// no answer: said as such by errors.ts
		xhr.onerror = xhr.ontimeout = () => reject(new ApiError("", 0, UNREACHABLE));
		xhr.onload = () => resolve({ status: xhr.status, data: xhr.response ?? {} });
		xhr.send(form);
	});
}

export const androidTvApi = {
	status: () => get<AndroidTvStatusResponse>("/androidtv"),
	setConfig: (config: AndroidTvConfig) => put("/androidtv/config", config),
	sendKey: (key: AndroidKey) => post("/androidtv/key", { key }),
	launch: (pkg: string, ensureTvOn = true) => post("/androidtv/launch", { package: pkg, ensureTvOn }),
	apps: () => get<{ success: boolean; packages: string[] }>("/androidtv/apps"),
	wake: () => post("/androidtv/wake"),
	sleep: () => post("/androidtv/sleep"),
	pairStart: () => post("/androidtv/pair/start"),
	pairFinish: (code: string) => post("/androidtv/pair/finish", { code }),

	/**
	 * Sideloads an APK onto the box. XMLHttpRequest rather than `fetch`: only it reports upload
	 * progress, and an APK is tens of MB on a Pi's link. `onProgress` gets the fraction sent (0
	 * to 1); at 1 the box is installing and the answer comes when `pm install` is done. An
	 * expired access token is renewed once, as for `api`, and the upload sent again.
	 */
	installApk: async (file: File, onProgress?: (sent: number) => void): Promise<SimpleResponse> => {
		let r = await upload(file, onProgress);
		if (r.status === 401) {
			if (await renewSession()) r = await upload(file, onProgress);
			if (r.status === 401) unauthorized();
		}
		if (r.status < 200 || r.status >= 300) throw new ApiError(r.data?.error ?? "", r.status);
		return r.data;
	},
};
