<script lang="ts">
	// The rabbit (garenne firmware): reachable or not, whether it shows Tempo, and a way to push
	// today's colour again. Nothing at all when no rabbit is configured.
	import { m } from '#lib/paraglide/messages.js';
	import { nabaztagApi } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';

	/** The rabbit's cadence (docs/ux/tableau-de-bord.md § 4: 120 s). */
	const POLL_MS = 120_000;
	const status = live('nabaztag', nabaztagApi.status, POLL_MS);
	const s = $derived(status.data);
	const pushing = new Gesture();
	const push = () =>
		pushing.run(
			() => nabaztagApi.pushTempo(),
			async () => {
				ui.toast(m.nabaztag_tempo_pushed());
				await status.refresh();
			}
		);
</script>

{#if s?.config.host}
	<section class="group" aria-labelledby="nabaztag-title">
		<h2 id="nabaztag-title" class="group-title">{m.nabaztag_name()}</h2>
		<div class="tiles">
			<DeviceTile
				name={m.nabaztag_rabbit()}
				icon="rabbit"
				on={s.reachable}
				warn={!s.reachable}
				state={!s.reachable ? m.state_unreachable() : s.config.tempoEnabled ? m.nabaztag_reachable_synced() : m.nabaztag_reachable()}
			>
				{#snippet end()}
					<button
						class="icon-btn"
						aria-label={m.nabaztag_push_tempo()}
						title={m.nabaztag_push_tempo()}
						aria-busy={pushing.is()}
						disabled={pushing.is() || !s.reachable}
						onclick={push}
					>
						<Icon name={pushing.is() ? 'loader-circle' : 'refresh-cw'} class={pushing.is() ? 'spin' : undefined} />
					</button>
				{/snippet}
			</DeviceTile>
		</div>
	</section>
{/if}
