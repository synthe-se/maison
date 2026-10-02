<script lang="ts">
	// A Zigbee lamp's page; keyed on the id so moving between two lamps starts afresh.
	import { page } from '$app/state';
	import ZigbeeLampDetail from '#lib/devices/lamps/ZigbeeLampDetail.svelte';
</script>

{#key page.params.lampId}
	<ZigbeeLampDetail id={page.params.lampId ?? ''} />
{/key}
