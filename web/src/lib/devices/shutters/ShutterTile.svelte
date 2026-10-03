<script lang="ts">
	// One shutter (docs/ux.md § 1–3): Open · Stop · Close, its position as « how open » (the
	// motor gets its target on release, the slider sending through its own gesture so an order
	// in flight never swallows a position), its next scheduled move and skipping it once
	// (« Ne pas fermer ce soir », then « Fermeture de 18:42 sautée · Annuler »), and under its
	// gear the name, the sun schedule and, for an admin, removing it. Out of reach, the buttons
	// stay, not operable, the state line saying why. Every answer names the shutter: the list
	// takes it at once.
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import type { Live } from '#lib/live.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic, TAP } from '#lib/haptics.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import RenameField from '#lib/components/RenameField.svelte';
	import { shuttersApi, type Shutter, type ShuttersResponse } from './api.ts';
	import { moving } from './data.ts';
	import { nextEvent, nextMove, skipLabel } from './sun.ts';
	import { at, describe } from './words.ts';
	import SunSettings from './SunSettings.svelte';

	interface Props {
		cover: Shutter;
		list: Live<ShuttersResponse>;
		/** Removed: its tile is gone, the group takes the focus. */
		onremoved: () => unknown;
	}
	let { cover: c, list, onremoved }: Props = $props();

	/** An answer that names the shutter: shown at once. */
	const replace = (cover: Shutter) => list.update((d) => ({ ...d, covers: d.covers.map((x) => (x.id === cover.id ? cover : x)) }));
	// keyed by what travels: a move, a skip, the removal
	const g = new Gesture();
	const next = $derived(nextEvent(c));

	function act(run: () => Promise<{ cover: Shutter }>, pattern = TAP) {
		haptic(pattern);
		return g.run(run, (r) => replace(r.cover), 'move');
	}

	/** Skips the next scheduled move once, or no longer (the undo). */
	function skip() {
		const e = nextEvent(c);
		if (!e) return;
		haptic(TAP);
		return g.run(
			() => shuttersApi.skip(c.id, e.event, !e.skipped),
			(r) => {
				replace(r.cover);
				ui.say(nextMove(r.cover) ?? '');
			},
			'skip'
		);
	}

	const remove = () =>
		g.run(
			() => shuttersApi.remove(c.id),
			async () => {
				ui.toast(m.shutters_removed({ name: c.name }));
				await list.refresh();
				await onremoved();
			},
			'remove'
		);
</script>

<DeviceTile name={c.name} icon="blinds" state={describe(c)} warn={!c.online} on={c.online && (c.openPercent ?? 0) > 0}>
	{#snippet children(why)}
		{@const off = !c.online && why}
		<div class="btn-row">
			<button class="btn" {...off ? unavailable(off) : pending(g.is('move'))} onclick={() => c.online && act(() => shuttersApi.open(c.id))}>
				<Icon name="arrow-up" /><span class="btn-text">{m.shutters_open_action()}</span>
			</button>
			<!-- Stop always goes, even while another order travels -->
			<button
				class="btn"
				class:primary={moving(c)}
				{...unavailable(off)}
				onclick={() => c.online && act(() => shuttersApi.stop(c.id), CONFIRM)}
			>
				<Icon name="square" /><span class="btn-text">{m.common_stop()}</span>
			</button>
			<button
				class="btn"
				{...off ? unavailable(off) : pending(g.is('move'))}
				onclick={() => c.online && act(() => shuttersApi.close(c.id))}
			>
				<Icon name="arrow-down" /><span class="btn-text">{m.shutters_close_action()}</span>
			</button>
		</div>

		{#if next}
			<!-- the next scheduled move, and skipping it once (or the undo) -->
			<p class="next hint" class:skipped={next.skipped}>
				<span>{nextMove(c)}</span> ·
				<button class="link-btn" {...pending(g.is('skip'))} onclick={skip}>
					{next.skipped ? m.shutters_skip_undo() : skipLabel(next.event, next.at)}
				</button>
			</p>
		{/if}

		<!-- percentage moves need the travel the switch learns while calibrating -->
		{#if c.online && c.openPercent !== null}
			<Range
				label={m.shutters_position()}
				hideLabel
				value={c.targetOpenPercent ?? c.openPercent}
				mark={moving(c) ? c.openPercent : undefined}
				step={5}
				page={5}
				ends={[m.shutters_closed(), m.shutters_open()]}
				valueText={at}
				send={(v) => shuttersApi.setPosition(c.id, v).then((r) => replace(r.cover))}
			/>
		{:else if c.online}
			<p class="hint">{m.shutters_calibrate_hint()}</p>
		{/if}
	{/snippet}

	{#snippet settings()}
		<RenameField
			label={m.common_name()}
			value={c.name}
			save={(n) => shuttersApi.rename(c.id, n).then((r) => replace(r.cover))}
			said={(n) => m.common_renamed({ name: n })}
		/>
		<SunSettings cover={c} onchange={replace} />
		<AdminOnly reason={false}>
			<div class="actions">
				<ConfirmDialog
					danger
					icon="trash"
					label={m.common_remove()}
					ariaLabel={m.common_remove_named({ name: c.name })}
					title={m.common_remove_from_maison({ name: c.name })}
					description={m.shutters_remove_description()}
					action={m.shutters_remove_action()}
					busy={g.is('remove')}
					onconfirm={remove}
				/>
			</div>
		</AdminOnly>
	{/snippet}
</DeviceTile>

<style>
	.next {
		font-variant-numeric: tabular-nums;
	}
	.next .link-btn {
		min-height: var(--control-h-xs);
	}
</style>
