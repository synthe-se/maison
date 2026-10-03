<script lang="ts">
	// Roller shutters over Matter. The list polls slowly at rest and every second while a
	// motor runs, so the position follows the travel. Positions are « how open » (100 = open).
	// Adding (commissioning) and removing a shutter are an admin's; moving, renaming and the
	// sun schedule are everyone's.
	import { m } from '#lib/paraglide/messages.js';
	import { shuttersApi, type Shutter } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { CONFIRM, haptic, TAP } from '#lib/haptics.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import RenameField from '#lib/components/RenameField.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import SunSettings from './shutters/SunSettings.svelte';
	import { nextMove } from './shutters/sun.ts';

	const moving = (c: Shutter) => c.motion === 'opening' || c.motion === 'closing';
	const list = live('shutters', shuttersApi.list, (d) => (d?.covers.some(moving) ? 1_000 : 10_000));
	const covers = $derived(list.data?.covers ?? []);

	let adding = $state(false);
	let code = $state('');
	let name = $state('');
	/** What the form says is missing, under its field. */
	let missing = $state<{ code?: string; name?: string }>({});
	/** The shutter whose settings are open. */
	let settingsOf = $state<string | null>(null);
	// two gestures: commissioning takes a while, and a shutter may be moved meanwhile without
	// clearing the form's mark. `g` is keyed by the shutter a command is travelling to.
	const pairing = new Gesture();
	const g = new Gesture();
	let title = $state<HTMLElement>();
	let addButton = $state<HTMLButtonElement>();
	let codeInput = $state<HTMLInputElement>();
	let nameInput = $state<HTMLInputElement>();

	// commissioning is one call of up to a minute or so: its elapsed time, said on screen (§ 6)
	let startedAt = $state(0);
	let now = $state(0);
	$effect(() => {
		if (!pairing.is()) return;
		const timer = setInterval(() => (now = Date.now()), 1_000);
		return () => clearInterval(timer);
	});
	const elapsed = $derived(pairing.is() ? Math.max(0, Math.round((now - startedAt) / 1000)) : 0);

	function replace(cover: Shutter) {
		if (list.data) list.set({ ...list.data, covers: list.data.covers.map((c) => (c.id === cover.id ? cover : c)) });
	}

	function act(c: Shutter, run: () => Promise<{ cover: Shutter }>, pattern = TAP) {
		haptic(pattern);
		return g.run(run, (r) => replace(r.cover), c.id);
	}

	function pair(e: SubmitEvent) {
		e.preventDefault();
		missing = { code: code.trim() ? undefined : m.shutters_code_missing(), name: name.trim() ? undefined : m.shutters_name_missing() };
		if (missing.code || missing.name) return void (missing.code ? codeInput : nameInput)?.focus();
		startedAt = now = Date.now();
		return pairing.run(
			() => shuttersApi.commission(code.trim(), name.trim()),
			async (r) => {
				haptic(CONFIRM);
				ui.toast(m.shutters_added({ name: r.cover.name }));
				adding = false;
				code = name = '';
				await list.refresh();
				// the form is gone: the focus goes back to the button that opened it
				await refocus(addButton, title);
			},
			'pair',
			// a refusal is most often the code (wrong, or the window closed): said under it
			{ field: () => codeInput }
		);
	}

	function remove(c: Shutter) {
		return g.run(
			() => shuttersApi.remove(c.id),
			async () => {
				ui.toast(m.shutters_removed({ name: c.name }));
				settingsOf = null;
				await list.refresh();
				// its tile is gone: the focus goes to the group's title
				await refocus(title);
			},
			c.id
		);
	}

	function describe(c: Shutter): string {
		if (!c.online) return m.state_unreachable();
		const going = c.targetOpenPercent !== null && c.openPercent !== null && c.targetOpenPercent !== c.openPercent;
		if (moving(c) && going) return `${at(c.openPercent!)} · ${m.shutters_going_to({ percent: c.targetOpenPercent! })}`;
		if (c.motion === 'opening') return m.shutters_opening();
		if (c.motion === 'closing') return m.shutters_closing();
		if (c.openPercent === null) return m.shutters_uncalibrated();
		return at(c.openPercent);
	}

	/** « Fermé », « Ouvert à 40 % », « Ouvert ». */
	function at(percent: number): string {
		if (percent >= 100) return m.shutters_open();
		if (percent <= 0) return m.shutters_closed();
		return m.shutters_open_percent({ percent });
	}
</script>

<section class="group" aria-labelledby="shutters-title">
	<div class="group-head">
		<h2 id="shutters-title" class="group-title" tabindex="-1" bind:this={title}>{m.shutters_title()}</h2>
		<AdminOnly reason={false}>
			<button class="btn fact-btn" aria-expanded={adding} bind:this={addButton} onclick={() => (adding = !adding)}>
				<Icon name="plus" />{m.common_add()}
			</button>
		</AdminOnly>
	</div>

	{#if adding}
		<form class="tile measure" onsubmit={pair} novalidate aria-labelledby="shutters-title">
			<p class="hint">{m.shutters_commission_hint()}</p>
			<div class="field">
				<label for="shutter-code">{m.shutters_code()}</label>
				<input
					id="shutter-code"
					bind:this={codeInput}
					bind:value={code}
					inputmode="numeric"
					autocomplete="off"
					placeholder="3497-011-2332"
					aria-invalid={missing.code || pairing.error ? 'true' : undefined}
					aria-describedby="shutter-code-error"
				/>
				<p class="form-error" id="shutter-code-error">{missing.code || pairing.error}</p>
			</div>
			<div class="field">
				<label for="shutter-name">{m.shutters_name()}</label>
				<input
					id="shutter-name"
					bind:this={nameInput}
					bind:value={name}
					placeholder={m.shutters_name_placeholder()}
					aria-invalid={missing.name ? 'true' : undefined}
					aria-describedby="shutter-name-error"
				/>
				<p class="form-error" id="shutter-name-error">{missing.name}</p>
			</div>
			<div class="actions">
				<button class="btn primary" {...pending(pairing.is())}>
					<Icon name="plus" busy={pairing.is()} />{m.action_pair()}
				</button>
				{#if pairing.is()}<span class="hint">{m.shutters_commissioning_elapsed({ seconds: elapsed })}</span>{/if}
			</div>
		</form>
	{/if}

	<Loaded value={list} empty={covers.length === 0 && !adding} emptyText={m.shutters_none()} emptyHint={m.shutters_none_hint()}>
		<div class="tiles">
			{#each covers as c (c.id)}
				<DeviceTile name={c.name} icon="blinds" state={describe(c)} warn={!c.online} on={c.online && (c.openPercent ?? 0) > 0} fact={nextMove(c)}>
					{#snippet end()}
						<button
							class="icon-btn"
							aria-label={m.common_settings_of({ name: c.name })}
							aria-expanded={settingsOf === c.id}
							onclick={() => (settingsOf = settingsOf === c.id ? null : c.id)}><Icon name="settings-2" /></button
						>
					{/snippet}
					<div class="btn-row">
						<button class="btn" disabled={!c.online} {...pending(g.is(c.id))} onclick={() => act(c, () => shuttersApi.open(c.id))}>
							<Icon name="arrow-up" /><span class="btn-text">{m.shutters_open_action()}</span>
						</button>
						<!-- Stop always goes, even while another order travels -->
						<button class="btn" class:primary={moving(c)} disabled={!c.online} onclick={() => act(c, () => shuttersApi.stop(c.id), CONFIRM)}>
							<Icon name="square" /><span class="btn-text">{m.common_stop()}</span>
						</button>
						<button class="btn" disabled={!c.online} {...pending(g.is(c.id))} onclick={() => act(c, () => shuttersApi.close(c.id))}>
							<Icon name="arrow-down" /><span class="btn-text">{m.shutters_close_action()}</span>
						</button>
					</div>

					<!-- percentage moves need the travel the switch learns while calibrating -->
					{#if c.online && c.openPercent !== null}
						<Range
							label={m.shutters_position()}
							value={c.targetOpenPercent ?? c.openPercent}
							mark={moving(c) ? c.openPercent : undefined}
							step={5}
							page={5}
							ends={[m.shutters_closed(), m.shutters_open()]}
							valueText={at}
							send={(v) => act(c, () => shuttersApi.setPosition(c.id, v))}
						/>
					{:else if c.online}
						<p class="hint">{m.shutters_calibrate_hint()}</p>
					{/if}

					{#if settingsOf === c.id}
						<div class="inset">
							<RenameField label={m.shutters_name()} value={c.name} save={(n) => shuttersApi.rename(c.id, n).then((r) => replace(r.cover))} said={(n) => m.common_renamed({ name: n })} />
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
										pending={g.is(c.id)}
										onconfirm={() => remove(c)}
									/>
								</div>
							</AdminOnly>
						</div>
					{/if}
				</DeviceTile>
			{/each}
		</div>
	</Loaded>
</section>
