<script lang="ts">
	// The Android TV box's row in the dashboard's « Télé » group: its icon wakes it or puts it to
	// sleep (the TV follows over CEC), its name leads to its page (/androidtv: the pad, the keys,
	// pairing and APKs), and its app chips (docs/ux.md § 1).
	import Loaded from '#lib/components/Loaded.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Apps from './Apps.svelte';
	import PairedChip from './PairedChip.svelte';
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
					<PairedChip paired={b.status?.paired} />
				{/snippet}
				{#if b.configured}<Apps box={b} />{/if}
			</DeviceTile>
		{/if}
	</div>
</Loaded>
