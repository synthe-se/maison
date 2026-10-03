<script lang="ts">
	// The rabbit (garenne firmware, « Garenne » in the house): reachable or not, whether it shows
	// Tempo, a way to push today's color again, and its tai chi (the choreography the remote's
	// « Taichi » key plays too). Out of reach, both buttons stay, not operable, the state line
	// saying why. Its place is kept while it is first asked and when the server does not
	// answer; nothing at all once the server says no rabbit is set.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { nabaztagApi } from './nabaztag/api.ts';
	import { rabbit } from './nabaztag/data.ts';

	/** The tai chi choreography, as stored on the rabbit (the remote key sends the same). */
	const TAICHI = 'chor /vl/config/chor/taichi.chor';
	const status = live(rabbit);
	const s = $derived(status.data);
	const pushing = new Gesture();
	const taichi = new Gesture();
	const dance = () =>
		taichi.run(
			() => nabaztagApi.command(TAICHI),
			() => ui.toast(m.nabaztag_taichi_done())
		);
	const push = () =>
		pushing.run(
			() => nabaztagApi.pushTempo(),
			async () => {
				ui.toast(m.nabaztag_tempo_pushed());
				await status.refresh();
			}
		);
</script>

{#if !s || s.config.host}
	<Group id="nabaztag-title" title={m.nabaztag_name()}>
		<Loaded value={status} skeletons={1}>
			{#if s}
				<div class="tiles">
					<DeviceTile
						name={m.nabaztag_rabbit()}
						icon="rabbit"
						on={s.reachable}
						warn={!s.reachable}
						state={!s.reachable ? m.state_unreachable() : s.config.tempoEnabled ? m.nabaztag_reachable_synced() : m.nabaztag_reachable()}
					>
						{#snippet end(why)}
							{@const off = !s.reachable && why}
							<button class="btn" {...off ? unavailable(off) : pending(taichi.is())} onclick={() => s.reachable && dance()}>
								<Icon name="rabbit" busy={taichi.is()} />{m.nabaztag_taichi()}
							</button>
							<button
								class="icon-btn"
								aria-label={m.nabaztag_push_tempo()}
								title={m.nabaztag_push_tempo()}
								{...off ? unavailable(off) : pending(pushing.is())}
								onclick={() => s.reachable && push()}
							>
								<Icon name="refresh-cw" busy={pushing.is()} />
							</button>
						{/snippet}
					</DeviceTile>
				</div>
			{/if}
		</Loaded>
	</Group>
{/if}
