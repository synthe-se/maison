<script lang="ts">
	// « Cadre photo » (/photos): an admin's, reached from « Mon compte », not a destination (docs/ux.md
	// § 5). A member reads why not, and nothing is asked of the server.
	import { m } from '#lib/paraglide/messages.js';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import PhotosPanel from './PhotosPanel.svelte';
</script>

<PageHead title={m.photos_title()} back>
	{#snippet sub()}{m.photos_subtitle()}{/snippet}
</PageHead>

<AdminOnly>
	<PhotosPanel />
</AdminOnly>
