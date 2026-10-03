<script lang="ts">
	// The Android TV box's row in the dashboard's « Télé » group: its icon wakes it or puts it to
	// sleep (the TV follows over CEC), its name leads to its page (/androidtv: the pad, the keys,
	// pairing and APKs), and its app chips (docs/ux.md § 1).
	import { m } from '#lib/paraglide/messages.js';
	import Loaded from '#lib/components/Loaded.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Apps from './Apps.svelte';
	import { Box } from './box.svelte.ts';

	const b = new Box();
</script>

<Loaded value={b.box} skeletons={1}>
	<div class="tiles">
		{#if b.status}
			<DeviceTile
				name={b.name}
				icon="play"
				href="/androidtv"
				state={b.state}
				on={b.configured ? b.status.awake : undefined}
				offline={b.configured && !b.reachable}
				command={b.awake}
				ontoggle={b.configured ? b.toggle : undefined}
			>
				{#snippet end()}
					<!-- which channel the keys take: the difference between 8 ms and 150 ms a press -->
					{#if b.status?.paired}<span class="chip accent"><Icon name="zap" size={12} />{m.android_tv_paired()}</span>{/if}
				{/snippet}
				{#if b.configured}<Apps box={b} />{/if}
			</DeviceTile>
		{/if}
	</div>
</Loaded>
