<script lang="ts">
	// Roller shutters over Matter. The list polls slowly at rest and every second while a
	// motor runs, so the position follows the travel. Positions are « how open » (100 = open).
	import { m } from '#lib/paraglide/messages.js';
	import { shuttersApi, type Shutter } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { CONFIRM, haptic, TAP } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import SunSettings from './shutters/SunSettings.svelte';
	import { nextMove } from './shutters/sun.ts';

	const moving = (c: Shutter) => c.motion === 'opening' || c.motion === 'closing';
	const list = live('shutters', shuttersApi.list, (d) => (d?.covers.some(moving) ? 1_000 : 10_000));
	const covers = $derived(list.data?.covers ?? []);

	let adding = $state(false);
	let code = $state('');
	let name = $state('');
	/** The shutter whose settings are open. */
	let settingsOf = $state<string | null>(null);
	// two gestures: commissioning takes a while, and a shutter may be moved meanwhile without
	// clearing the form's mark. `g` is keyed by the shutter a command is travelling to.
	const pairing = new Gesture();
	const g = new Gesture();
	let renameDraft = $state('');

	function replace(cover: Shutter) {
		if (list.data) list.set({ ...list.data, covers: list.data.covers.map((c) => (c.id === cover.id ? cover : c)) });
	}

	function act(c: Shutter, run: () => Promise<{ cover: Shutter }>, pattern = TAP) {
		haptic(pattern);
		return g.run(run, (r) => replace(r.cover), c.id);
	}

	function pair(e: SubmitEvent) {
		e.preventDefault();
		return pairing.run(
			() => shuttersApi.commission(code, name),
			async (r) => {
				haptic(CONFIRM);
				ui.toast(m.shutters_added({ name: r.cover.name }));
				adding = false;
				code = name = '';
				await list.refresh();
			}
		);
	}

	async function rename(c: Shutter) {
		await act(c, () => shuttersApi.rename(c.id, renameDraft));
		settingsOf = null;
	}

	function remove(c: Shutter) {
		return g.run(
			() => shuttersApi.remove(c.id),
			async () => {
				ui.toast(m.shutters_removed({ name: c.name }));
				await list.refresh();
			},
			c.id
		);
	}

	function describe(c: Shutter): string {
		if (!c.online) return m.state_unreachable();
		if (c.motion === 'opening') return m.shutters_opening();
		if (c.motion === 'closing') return m.shutters_closing();
		if (c.openPercent === null) return m.shutters_uncalibrated();
		if (c.openPercent === 100) return m.shutters_open();
		if (c.openPercent === 0) return m.shutters_closed();
		return m.shutters_open_percent({ percent: c.openPercent });
	}
</script>

<section class="group" aria-labelledby="shutters-title">
	<div class="group-head">
		<h2 id="shutters-title" class="group-title">{m.shutters_title()}</h2>
		<button class="btn fact-btn" aria-expanded={adding} onclick={() => (adding = !adding)}>
			<Icon name="plus" />{m.common_add()}
		</button>
	</div>

	{#if adding}
		<form class="tile measure" onsubmit={pair}>
			<p class="hint">{m.shutters_commission_hint()}</p>
			<div class="field">
				<label for="shutter-code">{m.shutters_code()}</label>
				<input id="shutter-code" bind:value={code} inputmode="numeric" autocomplete="off" placeholder="3497-011-2332" required />
			</div>
			<div class="field">
				<label for="shutter-name">{m.shutters_name()}</label>
				<input id="shutter-name" bind:value={name} placeholder={m.shutters_name_placeholder()} required />
			</div>
			<div class="actions">
				<button class="btn primary" disabled={pairing.is() || !code.trim() || !name.trim()}>
					{#if pairing.is()}<Icon name="loader-circle" class="spin" />{/if}{m.action_pair()}
				</button>
				{#if pairing.is()}<span class="hint" role="status">{m.shutters_commissioning()}</span>{/if}
			</div>
		</form>
	{/if}

	{#if list.loading}
		<p class="hint" role="status">{m.common_loading()}</p>
	{:else if covers.length === 0 && !adding}
		<div class="empty">
			<p>{m.shutters_none()}</p>
			<p class="hint">{m.shutters_none_hint()}</p>
		</div>
	{:else}
		<div class="tiles">
			{#each covers as c (c.id)}
				<DeviceTile name={c.name} icon="blinds" state={describe(c)} warn={!c.online} on={c.online && (c.openPercent ?? 0) > 0} fact={nextMove(c)}>
					{#snippet end()}
						<button
							class="icon-btn"
							aria-label={m.common_settings()}
							aria-expanded={settingsOf === c.id}
							onclick={() => {
								settingsOf = settingsOf === c.id ? null : c.id;
								renameDraft = c.name;
							}}><Icon name="settings-2" /></button
						>
					{/snippet}
					<div class="btn-row">
						<button class="btn" disabled={!c.online || g.is(c.id)} onclick={() => act(c, () => shuttersApi.open(c.id))}>
							<Icon name="arrow-up" />{m.shutters_open_action()}
						</button>
						<button
							class="btn"
							class:primary={moving(c)}
							disabled={!c.online}
							onclick={() => act(c, () => shuttersApi.stop(c.id), CONFIRM)}
						>
							<Icon name="square" />{m.shutters_stop()}
						</button>
						<button class="btn" disabled={!c.online || g.is(c.id)} onclick={() => act(c, () => shuttersApi.close(c.id))}>
							<Icon name="arrow-down" />{m.shutters_close_action()}
						</button>
					</div>

					<!-- percentage moves need the travel the switch learns while calibrating -->
					{#if c.online && c.openPercent !== null}
						<Range
							label={m.shutters_position()}
							value={c.targetOpenPercent ?? c.openPercent}
							step={5}
							valueText={(v) => m.shutters_open_percent({ percent: v })}
							page={5}
							send={(v) => act(c, () => shuttersApi.setPosition(c.id, v))}
						/>
					{:else if c.online}
						<p class="hint">{m.shutters_calibrate_hint()}</p>
					{/if}

					{#if settingsOf === c.id}
						<div class="inset">
							<div class="field">
								<label for="rename-{c.id}">{m.shutters_name()}</label>
								<div class="actions">
									<input id="rename-{c.id}" bind:value={renameDraft} />
									<button class="btn" disabled={!renameDraft.trim() || renameDraft === c.name} onclick={() => rename(c)}>
										{m.common_save()}
									</button>
								</div>
							</div>
							<SunSettings cover={c} onchange={replace} />
							<div class="actions">
								<ConfirmDialog
									danger
									icon="trash"
									label={m.common_remove()}
									title={m.shutters_remove_confirm({ name: c.name })}
									description={m.shutters_remove_description()}
									action={m.common_remove()}
									onconfirm={() => remove(c)}
								/>
							</div>
						</div>
					{/if}
				</DeviceTile>
			{/each}
		</div>
	{/if}
</section>

<style>
	.fact-btn { margin-left: auto; }
	.inset input { flex: 1; min-width: 0; }
</style>
