<script lang="ts">
	// The dashboard's way to the remote configurator: how many buttons do something, and a link.
	import { m } from '#lib/paraglide/messages.js';
	import { irApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';

	// the keymap only changes from the configurator, which refreshes it: no polling
	const keymap = live('ir-keymap', irApi.keymap);
	const count = $derived(Object.keys(keymap.data?.keymap ?? {}).length);
</script>

<section class="group" aria-labelledby="remote-link-title">
	<h2 id="remote-link-title" class="group-title">{m.remote_title()}</h2>
	<div class="tiles">
		<DeviceTile
			name={m.remote_stb_name()}
			icon="radio"
			href="/remote"
			state={keymap.loading ? m.common_loading() : count ? m.remote_binding_count({ count }) : m.remote_no_bindings()}
		>
			<div class="actions">
				<a class="btn" href="/remote"><Icon name="settings" />{m.remote_configure()}</a>
			</div>
		</DeviceTile>
	</div>
</section>

<style>
	.btn { min-height: var(--control-h); }
</style>
