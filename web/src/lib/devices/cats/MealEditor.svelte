<script lang="ts">
	// One scheduled meal: its time, its portions, its days, on or off. Keyboard: Tab through
	// the time, the slider (arrows), the days (Space toggles), the switch, then Save.
	import { m } from '#lib/paraglide/messages.js';
	import type { MealPlanEntry } from '#lib/api.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { DAYS, WEEKDAYS, WEEKEND, dayName } from './meals.ts';

	interface Props {
		meal?: MealPlanEntry;
		onsave: (meal: MealPlanEntry) => void;
		oncancel: () => void;
		/** Changed from what it opened with (the sheet asks before losing it). */
		dirty?: boolean;
	}
	let { meal, onsave, oncancel, dirty = $bindable(false) }: Props = $props();
	/** What the feeder accepts in one meal. */
	const MAX_PORTIONS = 12;
	const DEFAULT_TIME = '08:00';

	const id = $props.id();
	// a draft: the meal as it was when the dialog opened (the dialog remounts the editor)
	// svelte-ignore state_referenced_locally
	let time = $state(meal?.time ?? DEFAULT_TIME);
	// svelte-ignore state_referenced_locally
	let portion = $state(meal?.portion ?? 1);
	// svelte-ignore state_referenced_locally
	let days = $state<string[]>([...(meal?.days_of_week ?? DAYS)]);
	// svelte-ignore state_referenced_locally
	let enabled = $state(meal?.status !== 'Disabled');
	let error = $state('');
	// svelte-ignore state_referenced_locally
	const opened = JSON.stringify([time, portion, days, enabled]);
	$effect(() => {
		dirty = JSON.stringify([time, portion, days, enabled]) !== opened;
	});

	const toggleDay = (d: string) => {
		days = days.includes(d) ? days.filter((x) => x !== d) : [...days, d];
		error = '';
	};
	const PRESETS = [
		{ label: m.common_all, days: DAYS },
		{ label: m.meal_plan_weekdays_only, days: WEEKDAYS },
		{ label: m.meal_plan_weekend, days: WEEKEND }
	];

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (days.length === 0) {
			error = m.meal_plan_select_at_least_one_day();
			ui.say(error);
			return;
		}
		onsave({ time, portion, days_of_week: DAYS.filter((d) => days.includes(d)), status: enabled ? 'Enabled' : 'Disabled' });
	}
</script>

<form class="editor" onsubmit={submit}>
	<div class="field">
		<label for="{id}-time">{m.meal_plan_time()}</label>
		<input id="{id}-time" class="time" type="time" bind:value={time} required />
	</div>

	<div class="field">
		<Range
			label={m.feeder_portions()}
			value={portion}
			min={1}
			max={MAX_PORTIONS}
			valueText={(count) => m.feeder_portion({ count })}
			oncommit={(v) => (portion = v)}
		/>
		<p class="hint">{m.meal_plan_portions_per_meal()}</p>
	</div>

	<fieldset class="field" aria-describedby={error ? `${id}-error` : undefined}>
		<legend class="label">{m.meal_plan_days()}</legend>
		<div class="days">
			{#each DAYS as d (d)}
				<button type="button" class="btn day" aria-pressed={days.includes(d)} onclick={() => toggleDay(d)}>
					<span aria-hidden="true">{dayName(d, 'short')}</span><span class="sr-only">{dayName(d, 'long')}</span>
				</button>
			{/each}
		</div>
		<div class="actions">
			{#each PRESETS as p (p.days)}
				<button type="button" class="btn ghost" onclick={() => ((days = [...p.days]), (error = ''))}>{p.label()}</button>
			{/each}
		</div>
		<p class="form-error" id="{id}-error">{error}</p>
	</fieldset>

	<div class="enabled">
		<Toggle label={m.meal_plan_enable_meal()} checked={enabled} onchange={(v) => (enabled = v)} />
		<p class="hint">{m.meal_plan_disable_temporarily()}</p>
	</div>

	<div class="actions end">
		<button type="button" class="btn" onclick={oncancel}>{m.common_cancel()}</button>
		<button class="btn primary"><Icon name="save" />{m.common_save()}</button>
	</div>
</form>

<style>
	.editor { display: grid; gap: var(--s-5); }
	.time { font-size: 1.5rem; font-variant-numeric: tabular-nums; text-align: center; }
	fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
	legend { padding: 0; margin-bottom: 6px; font-weight: 600; font-size: 0.85rem; }
	/* seven equal days, the width of a phone (390 px) without scrolling */
	.days { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: var(--s-1); }
	.day { min-height: var(--control-h); padding: 0; justify-content: center; font-size: 0.8rem; }
	.day[aria-pressed='true'] { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
	.enabled { display: grid; gap: var(--s-1); }
	.enabled :global(.toggle) { display: flex; justify-content: space-between; }
	.actions.end .btn { min-height: var(--control-h); }
</style>
