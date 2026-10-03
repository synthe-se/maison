<script lang="ts">
	import { m } from '#lib/paraglide/messages.js';
</script>

<a class="brand" href="/" aria-label={m.branding_name()}>
	<img src="/brand.svg" alt="" width="28" height="28" />
	<span class="name" aria-hidden="true">{m.branding_name()}</span>
</a>

<style>
	.brand {
		display: inline-flex;
		align-items: center;
		gap: var(--s-2);
		min-height: var(--control-h);
		color: var(--ink);
		text-decoration: none;
	}
	/* the name only, in Borel (as Ariane): never a title, never body text */
	.name {
		font: var(--t-brand);
		padding-top: 0.3em;
	}
</style>
