<script lang="ts">
	// The Mitsubishi air conditioner, driven in infrared through the Broadlink RM4 Pro. The unit
	// never says its state (docs/ux.md § 2, case 3): two buttons, Allumer and Éteindre, and the
	// last command the house sent. The settings are a form under the tile's gear (ClimateForm);
	// the last command the server persisted fills it once, from its first answer (AGENTS.md
	// « Climate »). Without a blaster on the network the buttons stay, not operable, saying why.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { when } from '#lib/i18n.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import { broadlinkApi } from './climate/api.ts';
	import { CLIMATE_OFF } from './climate/command.ts';
	import { blasters, climateState, searchOver, SEARCH_FOR_MS } from './climate/data.ts';
	import { commandOf, formFrom, INITIAL } from './climate/form.ts';
	import { degrees, modeLabel } from './climate/labels.ts';
	import ClimateForm from './climate/ClimateForm.svelte';

	/** The model the backend encodes for (the living-room unit). */
	const MODEL = 'msz-hj5va';

	const found = live(blasters);
	const stored = live(climateState);
	const remote = $derived(found.data?.devices[0]);
	const last = $derived(stored.data?.state ?? null);

	let form = $state({ ...INITIAL });
	// once, from the first answer: re-applying the stored state later would clobber an edit
	void stored.ready.then((r) => (form = formFrom(r.state?.settings)));
	const command = $derived(commandOf(form));

	/** Keyed by the command travelling to the unit. */
	const sending = new Gesture();
	/** A fresh search of the network (an admin's: the server scans again). */
	const finder = new Gesture();
	const search = () => finder.run(() => found.refresh(true), undefined, 'search');

	// no blaster within the search's 2 min: said, and the polls stop (the search button asks again)
	let over = $state(false);
	$effect(() => {
		if (remote) return void (over = false);
		const left = found.since + SEARCH_FOR_MS - Date.now();
		over = searchOver(found.data, found.since);
		if (over) return;
		const t = setTimeout(() => (over = true), left);
		return () => clearTimeout(t);
	});

	function send(cmd: string) {
		if (!remote) return;
		const { host } = remote;
		return sending.run(
			() => broadlinkApi.sendMitsubishiCommand(host, cmd, MODEL),
			async () => {
				ui.say(m.climate_command_sent());
				await stored.refresh();
			},
			cmd
		);
	}

	const lastLine = $derived(
		!last
			? m.climate_no_order()
			: last.power
				? m.climate_last_on({ when: when(last.updatedAt) })
				: m.climate_last_off({ when: when(last.updatedAt) })
	);
	const fact = $derived(
		last?.power && last.settings
			? m.climate_fact({ temperature: degrees(last.settings.temperature), mode: modeLabel(last.settings.mode) })
			: undefined
	);
	const id = $props.id();
	/** Why the buttons cannot act: no blaster found (yet). */
	const why = $derived(remote ? undefined : `${id}-why`);
</script>

<Group id="climate-title" title={m.climate_dashboard_title()} value={stored}>
	{#snippet actions()}
		<AdminOnly reason={false}>
			<button class="icon-btn" aria-label={m.climate_search_remote()} {...pending(finder.is())} onclick={search}>
				<Icon name="refresh-cw" busy={finder.is()} />
			</button>
		</AdminOnly>
	{/snippet}

	<div class="tiles">
		<DeviceTile name={m.climate_brand_name()} icon="snowflake" state={lastLine} {fact} warn={!remote && over}>
			{#snippet settings()}
				<ClimateForm bind:form {command} host={remote?.host} busy={sending.is()} {why} {send} />
			{/snippet}

			{#if !remote}
				{#if over}
					<div class="hint" id="{id}-why">
						<p class="warn-text">{m.climate_no_remote_title()}</p>
						<p>{m.climate_no_remote_description()}</p>
					</div>
				{:else}
					<p class="hint searching" id="{id}-why">
						<Icon name="search" busy />{m.climate_searching_title()} — {m.climate_searching_description()}
					</p>
				{/if}
			{/if}

			<div class="btn-row">
				<button class="btn" {...why ? unavailable(why) : pending(sending.is())} onclick={() => !why && !sending.is() && send(command)}>
					<Icon name="power" busy={sending.is(command)} /><span class="btn-text">{m.action_turn_on()}</span>
				</button>
				<button class="btn" {...why ? unavailable(why) : pending(sending.is())} onclick={() => !why && !sending.is() && send(CLIMATE_OFF)}>
					<Icon name="square" busy={sending.is(CLIMATE_OFF)} /><span class="btn-text">{m.action_turn_off()}</span>
				</button>
			</div>
		</DeviceTile>
	</div>
</Group>

<style>
	.searching {
		display: flex;
		align-items: center;
		gap: var(--s-2);
	}
	.hint p {
		margin: 0;
	}
</style>
