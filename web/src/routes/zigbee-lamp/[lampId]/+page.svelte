<script lang="ts">
	// A Zigbee lamp's page; keyed on the id so moving between two lamps starts afresh.
	import { page } from '$app/state';
	import ZigbeeLampPage from '#lib/devices/lamps/ZigbeeLampPage.svelte';
</script>

{#key page.params.lampId}
	<ZigbeeLampPage id={page.params.lampId ?? ''} />
{/key}
