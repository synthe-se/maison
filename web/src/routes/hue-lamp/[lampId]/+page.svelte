<script lang="ts">
	// A Hue lamp's page; keyed on the id so moving between two lamps starts afresh.
	import { page } from '$app/state';
	import HueLampPage from '#lib/devices/lamps/HueLampPage.svelte';
</script>

{#key page.params.lampId}
	<HueLampPage id={page.params.lampId ?? ''} />
{/key}
