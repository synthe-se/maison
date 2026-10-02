<script lang="ts">
	// The feeder's scheduled meals. Each change is sent at once (a switch takes effect, § 2);
	// a deletion is immediate with « Rétablir » for 10 s rather than a dialog (§ 6).
	import Sheet from '#lib/components/Sheet.svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { feederApi, type MealPlanEntry } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import MealEditor from './MealEditor.svelte';
	import { describeDays } from './meals.ts';
	import { MAX_MEALS, clockOf } from './tuya.svelte.ts';

	/** How long a deleted meal can be brought back (Material snackbar: 4 to 10 s). */
	const UNDO_MS = 10_000;

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const list = live(`tuya:${id}:meal-plan`, () => feederApi.getMealPlan(id));

	/** The plan being sent, shown until the feeder's answer replaces it. */
	let sending = $state<MealPlanEntry[] | null>(null);
	const plan = $derived(sending ?? list.data?.decoded ?? []);
	const rows = $derived(plan.map((meal, index) => ({ meal, index })).sort((a, b) => a.meal.time.localeCompare(b.meal.time)));
	const perDay = $derived(plan.filter((x) => x.status === 'Enabled').reduce((n, x) => n + x.portion, 0));

	/** The meal open in the editor: its index, `'new'`, or none. */
	let editing = $state<number | 'new' | null>(null);
	let undo = $state<{ plan: MealPlanEntry[]; time: string } | null>(null);
	let undoTimer: ReturnType<typeof setTimeout> | undefined;

	const g = new Gesture();

	async function save(next: MealPlanEntry[], said = m.meal_plan_meal_plan_saved()) {
		sending = next;
		await g.run(
			() => feederApi.setMealPlan(id, next),
			async () => {
				await list.refresh();
				ui.say(said);
			}
		);
		sending = null;
	}

	function put(meal: MealPlanEntry) {
		const at = editing;
		editing = null;
		void save(at === 'new' || at === null ? [...plan, meal] : plan.map((x, i) => (i === at ? meal : x)));
	}

	function remove(index: number) {
		const before = plan;
		const time = clockOf(plan[index].time);
		clearTimeout(undoTimer);
		undo = { plan: before, time };
		undoTimer = setTimeout(() => (undo = null), UNDO_MS);
		void save(
			plan.filter((_, i) => i !== index),
			m.meal_plan_deleted({ time })
		);
	}

	function restore() {
		if (!undo) return;
		clearTimeout(undoTimer);
		const back = undo.plan;
		undo = null;
		void save(back);
	}

	const toggle = (index: number, on: boolean) =>
		save(plan.map((x, i) => (i === index ? { ...x, status: on ? 'Enabled' : 'Disabled' } : x)));
</script>

<div class="meals">
	<div class="head">
		<p class="hint">{m.meal_plan_count({ count: plan.length })}</p>
		<button class="btn" disabled={plan.length >= MAX_MEALS || g.is()} onclick={() => (editing = 'new')}>
			<Icon name="plus" />{m.meal_plan_add_meal()}
		</button>
	</div>
	{#if plan.length >= MAX_MEALS}<p class="hint">{m.meal_plan_limit()}</p>{/if}

	{#if undo}
		<div class="callout undo">
			<p>{m.meal_plan_deleted({ time: undo.time })}</p>
			<button class="btn" onclick={restore}><Icon name="corner-up-left" />{m.meal_plan_restore()}</button>
		</div>
	{/if}

	{#if list.loading}
		<p class="hint" role="status">{m.common_loading()}</p>
	{:else if rows.length === 0}
		<div class="empty">
			<p>{m.meal_plan_no_meals()}</p>
			<p class="hint">{m.meal_plan_no_meals_description()}</p>
		</div>
	{:else}
		<ul class="list" aria-busy={g.is()}>
			{#each rows as { meal, index } (index)}
				{@const time = clockOf(meal.time)}
				<li class="meal" class:off={meal.status === 'Disabled'}>
					<span class="time">{time}</span>
					<div class="what">
						<span>{m.feeder_portion({ count: meal.portion })}</span>
						<span class="hint">{describeDays(meal.days_of_week)}</span>
					</div>
					<div class="ops">
						<Toggle
							label={m.meal_plan_meal({ time })}
							hideLabel
							checked={meal.status === 'Enabled'}
							pending={g.is()}
							onchange={(on) => toggle(index, on)}
						/>
						<button class="icon-btn op" aria-label={m.meal_plan_edit_label({ time })} disabled={g.is()} onclick={() => (editing = index)}>
							<Icon name="pencil" />
						</button>
						<button class="icon-btn op" aria-label={m.meal_plan_delete_label({ time })} disabled={g.is()} onclick={() => remove(index)}>
							<Icon name="trash" />
						</button>
					</div>
				</li>
			{/each}
		</ul>
		<p class="total">{m.meal_plan_total_portions_per_day({ total: m.feeder_portion({ count: perDay }) })}</p>
	{/if}
</div>

<Sheet
	open={editing !== null}
	onclose={() => (editing = null)}
	title={editing === 'new' ? m.meal_plan_add_meal() : m.meal_plan_edit_meal()}
	description={m.feeder_meal_schedule_description()}
>
	{#if editing !== null}
		<MealEditor meal={editing === 'new' ? undefined : plan[editing]} onsave={put} oncancel={() => (editing = null)} />
	{/if}
</Sheet>

<style>
	.meals { display: grid; gap: var(--s-3); }
	.head { display: flex; align-items: center; justify-content: space-between; gap: var(--s-3); flex-wrap: wrap; }
	.undo { grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
	.meal {
		display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--s-2) var(--s-3);
		min-height: var(--row-min); padding-block: var(--s-2); border-bottom: 1px solid var(--line);
	}
	.meal:last-child { border-bottom: 0; }
	.time { font: var(--t-group); font-variant-numeric: tabular-nums; }
	.what { display: grid; min-width: 0; }
	/* on/off, edit, delete: under the meal on a phone, at its end when there is room */
	.ops { grid-column: 1 / -1; display: flex; align-items: center; gap: var(--s-1); }
	.ops :global(.toggle) { margin-right: auto; }
	@container (min-width: 30rem) {
		.meal { grid-template-columns: auto minmax(0, 1fr) auto; }
		.ops { grid-column: auto; }
	}
	.meals { container-type: inline-size; }
	.op { width: var(--control-h); height: var(--control-h); }
	/* a meal switched off: said by its switch, dimmed only as an echo */
	.off .time, .off .what { color: var(--ink-muted); }
	.total { margin: 0; font: var(--t-secondary); color: var(--ink-muted); }
</style>
