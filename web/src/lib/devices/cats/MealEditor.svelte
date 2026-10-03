<script lang="ts">
	// One scheduled meal: its time, its portions, its days, on or off, in a draft the sheet
	// reads (it asks before losing a change). Keyboard: Tab through the time, the slider
	// (arrows), the days (arrows between them, Space toggles one), the switch, then Save.
	import { m } from '#lib/paraglide/messages.js';
	import { ui } from '#lib/ui.svelte.ts';
	import type { Draft } from '#lib/draft.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Range from '#lib/components/Range.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import ToggleGroup from '#lib/components/ToggleGroup.svelte';
	import type { MealPlanEntry } from './api.ts';
	import { DAYS, WEEKDAYS, WEEKEND, dayName, type Day, type MealForm } from './meals.ts';

	interface Props {
		draft: Draft<MealForm>;
		onsave: (meal: MealPlanEntry) => void;
		oncancel: () => void;
	}
	let { draft, onsave, oncancel }: Props = $props();
	/** What the feeder accepts in one meal. */
	const MAX_PORTIONS = 12;

	const id = $props.id();
	const form = $derived(draft.current);
	let error = $state('');

	const PRESETS = [
		{ label: m.common_all, days: DAYS },
		{ label: m.meal_plan_weekdays_only, days: WEEKDAYS },
		{ label: m.meal_plan_weekend, days: WEEKEND }
	];
	const setDays = (days: readonly Day[]) => {
		form.days = [...days];
		error = '';
	};

	function submit(e: SubmitEvent) {
		e.preventDefault();
		if (form.days.length === 0) {
			error = m.meal_plan_select_at_least_one_day();
			ui.say(error);
			return;
		}
		onsave({
			time: form.time,
			portion: form.portion,
			daysOfWeek: DAYS.filter((d) => form.days.includes(d)),
			status: form.enabled ? 'Enabled' : 'Disabled'
		});
	}
</script>

<form class="editor" onsubmit={submit}>
	<div class="field">
		<label for="{id}-time">{m.meal_plan_time()}</label>
		<input id="{id}-time" class="time" type="time" bind:value={form.time} required />
	</div>

	<div class="field">
		<Range
			label={m.feeder_portions()}
			value={form.portion}
			min={1}
			max={MAX_PORTIONS}
			valueText={(count) => m.feeder_portion({ count })}
			oncommit={(v) => (form.portion = v)}
		/>
		<p class="hint">{m.meal_plan_portions_per_meal()}</p>
	</div>

	<div class="field days">
		<ToggleGroup
			type="multiple"
			label={m.meal_plan_days()}
			describedby={error ? `${id}-error` : undefined}
			options={DAYS.map((d) => ({ value: d, label: dayName(d, 'long') }))}
			value={form.days}
			onchange={setDays}
		>
			{#snippet item(o)}<span aria-hidden="true">{dayName(o.value, 'short')}</span><span class="sr-only">{o.label}</span>{/snippet}
		</ToggleGroup>
		<div class="actions">
			{#each PRESETS as p (p.days)}
				<button type="button" class="btn ghost" onclick={() => setDays(p.days)}>{p.label()}</button>
			{/each}
		</div>
		<p class="form-error" id="{id}-error">{error}</p>
	</div>

	<div class="enabled">
		<Toggle label={m.meal_plan_enable_meal()} checked={form.enabled} onchange={(v) => (form.enabled = v)} />
		<p class="hint">{m.meal_plan_disable_temporarily()}</p>
	</div>

	<div class="actions end">
		<button type="button" class="btn" onclick={oncancel}>{m.common_cancel()}</button>
		<button class="btn primary"><Icon name="save" />{m.common_save()}</button>
	</div>
</form>

<style>
	.editor {
		display: grid;
		gap: var(--s-5);
	}
	.time {
		font: var(--t-page);
		font-variant-numeric: tabular-nums;
		text-align: center;
	}
	/* seven equal days, the width of a phone (390 px) without scrolling */
	.days :global(.segmented .btn) {
		padding: 0;
		font: var(--t-meta);
	}
	.enabled {
		display: grid;
		gap: var(--s-1);
	}
	.enabled :global(.toggle) {
		display: flex;
		justify-content: space-between;
	}
	.actions.end .btn {
		min-height: var(--control-h);
	}
</style>
