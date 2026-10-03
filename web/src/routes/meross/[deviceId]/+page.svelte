<script lang="ts">
	// A Meross plug's page; a new plug id is a new view (its own live values).
	import { page } from '$app/state';
	import PlugPage from '#lib/devices/meross/PlugPage.svelte';
</script>

{#key page.params.deviceId}
	<PlugPage id={page.params.deviceId ?? ''} />
{/key}
