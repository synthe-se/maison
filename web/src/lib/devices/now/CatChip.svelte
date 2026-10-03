<script lang="ts">
	// A cat device that needs a hand (« Fontaine : eau basse »), in the strip; nothing otherwise.
	import { m } from '#lib/paraglide/messages.js';
	import type { Device } from '#lib/devices/cats/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { inSentence } from '#lib/i18n.svelte.ts';
	import { status, type StatusKind } from '#lib/devices/cats/data.ts';
	import { statusLine } from '#lib/devices/cats/alerts.ts';
	import Chip from './Chip.svelte';

	let { device }: { device: Device } = $props();
	// svelte-ignore state_referenced_locally (one chip per device: the parent keys the list)
	const st = live(status(device.type as StatusKind, device.id));
	const line = $derived(device.connected ? statusLine(device.type, st.data) : null);
</script>

{#if line?.warn}<Chip text={m.now_alert({ name: device.name, what: inSentence(line.text) })} to="cats-title" warn />{/if}
