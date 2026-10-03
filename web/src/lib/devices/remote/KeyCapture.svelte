<script lang="ts">
	// A key being captured from the remote, mounted while capturing: the set-top box's last
	// events are read once a second (data.ts `recent`) and the newest press numbered after the
	// newest event seen when the capture started is the key. By the backend's sequence, never by
	// its clock: an NTP step back on the Pi (no RTC) would hide every new press. A sequence lower
	// than the baseline means the backend restarted: everything is new.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { recent } from './data.ts';

	interface Props {
		oncapture: (code: number) => void;
		/** The events could not be read: the capture stops, the reason said. */
		onerror: (e: Error) => void;
	}
	let { oncapture, onerror }: Props = $props();

	const events = live(recent);
	const started = Date.now();
	const failedBefore = events.error;
	/** undefined: not read yet in this capture; 0: the list was empty. */
	let baseline: number | undefined;
	let done = false;

	$effect(() => {
		const d = events.data;
		// an answer from before this capture (another one's) says nothing about this one
		if (done || !d || events.at < started) return;
		const newest = d.events[0]?.seq ?? 0;
		if (baseline === undefined) return void (baseline = newest);
		const since = newest < baseline ? 0 : baseline;
		const press = d.events.find((e) => e.seq > since && e.value === 1);
		if (!press) return;
		done = true;
		oncapture(press.code);
	});
	$effect(() => {
		if (!done && events.error && events.error !== failedBefore) {
			done = true;
			onerror(events.error);
		}
	});
</script>

<p class="hint">{m.remote_capturing()}</p>
