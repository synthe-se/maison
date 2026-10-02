<script lang="ts">
	// A Hue lamp's page; keyed on the id so moving between two lamps starts afresh.
	import { page } from '$app/state';
	import HueLampDetail from '#lib/devices/lamps/HueLampDetail.svelte';
</script>

{#key page.params.lampId}
	<HueLampDetail id={page.params.lampId ?? ''} />
{/key}
