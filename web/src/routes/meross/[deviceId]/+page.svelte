<script lang="ts">
	// A Meross plug's page; a new plug id is a new view (its own live values).
	import { page } from '$app/state';
	import PlugDetail from '#lib/devices/meross/PlugDetail.svelte';
</script>

{#key page.params.deviceId}
	<PlugDetail id={page.params.deviceId ?? ''} />
{/key}
