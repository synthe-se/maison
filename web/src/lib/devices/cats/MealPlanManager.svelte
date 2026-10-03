<script lang="ts">
	// The feeder's scheduled meals. Each change is sent at once (a switch takes effect, § 2);
	// a deletion is immediate with « Rétablir » in its toast rather than a dialog (§ 6): the focus
	// goes to it (the row pressed is gone), and back to the section's title (or « add ») when the
	// toast leaves. One whole-plan write at a time: another waits for the one travelling (two would
	// race), « Rétablir » included.
	import { m } from '#lib/paraglide/messages.js';
	import { hhmm } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import { refocus, sectionHeading } from '#lib/focus.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import Sheet from '#lib/components/Sheet.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { feederApi, type MealPlanEntry } from './api.ts';
	import { mealPlan } from './data.ts';
	import { MAX_MEALS } from './tuya.svelte.ts';
	import MealEditor from './MealEditor.svelte';
	import { describeDays, formOf, type MealForm } from './meals.ts';

	let { id }: { id: string } = $props();
	// svelte-ignore state_referenced_locally -- the page is keyed by device: `id` never changes here
	const list = live(mealPlan(id));

	/** The plan being sent, shown until the feeder's answer replaces it. */
	let sending = $state<MealPlanEntry[] | null>(null);
	const plan = $derived(sending ?? list.data?.decoded ?? []);
	const rows = $derived(plan.map((meal, index) => ({ meal, index })).toSorted((a, b) => a.meal.time.localeCompare(b.meal.time)));
	const perDay = $derived(plan.filter((x) => x.status === 'Enabled').reduce((n, x) => n + x.portion, 0));

	/** The meal open in the editor: its index, `'new'`, or none, and its draft. */
	let editing = $state<{ at: number | 'new'; draft: Draft<MealForm> } | null>(null);
	const edit = (at: number | 'new') => (editing = { at, draft: new Draft(formOf(at === 'new' ? undefined : plan[at])) });

	const g = new Gesture();
	const uid = $props.id();
	const full = $derived(plan.length >= MAX_MEALS);
	let addButton = $state<HTMLButtonElement>();
	/** The write travelling: the next one waits for it. */
	let writing: Promise<unknown> = Promise.resolve();

	function save(next: MealPlanEntry[], said: string | null = m.meal_plan_meal_plan_saved()) {
		const run = async () => {
			sending = next;
			await g.run(
				() => feederApi.setMealPlan(id, next),
				async () => {
					await list.refresh();
					if (said) ui.say(said);
				}
			);
			sending = null;
		};
		writing = writing.then(run);
		return writing;
	}

	function put(meal: MealPlanEntry) {
		const at = editing?.at;
		editing = null;
		void save(at === 'new' || at === undefined ? [...plan, meal] : plan.with(at, meal));
	}

	async function remove(index: number, e: Event) {
		const before = plan;
		const time = hhmm(plan[index].time);
		// where the focus goes once the toast leaves: the section's title, else « add »
		const heading = sectionHeading(e.currentTarget as HTMLElement) ?? addButton;
		const done = save(
			plan.filter((_, i) => i !== index),
			null
		);
		const toast = ui.toast(m.meal_plan_deleted({ time }), {
			action: { label: m.meal_plan_restore(), run: () => save(before), back: () => heading }
		});
		// the row and its button are gone: the focus goes to « Rétablir »
		await refocus(`#toast-action-${toast}`, heading);
		await done;
	}

	const toggle = (index: number, on: boolean) => save(plan.with(index, { ...plan[index], status: on ? 'Enabled' : 'Disabled' }));
</script>

<div class="meals">
	<div class="head">
		<p class="hint">{m.meal_plan_count({ count: plan.length })}</p>
		<button
			class="btn"
			bind:this={addButton}
			{...full ? unavailable(`${uid}-full`) : pending(g.is())}
			onclick={() => !full && !g.is() && edit('new')}
		>
			<Icon name="plus" />{m.meal_plan_add_meal()}
		</button>
	</div>
	{#if full}<p class="hint" id="{uid}-full">{m.meal_plan_limit()}</p>{/if}

	<Loaded value={list} empty={rows.length === 0} emptyText={m.meal_plan_no_meals()} emptyHint={m.meal_plan_no_meals_description()}>
		<ul class="plain-list" aria-busy={g.is()}>
			{#each rows as { meal, index } (index)}
				{@const time = hhmm(meal.time)}
				<li class="meal" class:off={meal.status === 'Disabled'}>
					<span class="time">{time}</span>
					<div class="what">
						<span>{m.feeder_portion({ count: meal.portion })}</span>
						<span class="hint">{describeDays(meal.daysOfWeek)}</span>
					</div>
					<div class="ops">
						<Toggle
							label={m.meal_plan_meal({ time })}
							hideLabel
							checked={meal.status === 'Enabled'}
							busy={g.is()}
							onchange={(on) => toggle(index, on)}
						/>
						<button
							class="icon-btn op"
							aria-label={m.meal_plan_edit_label({ time })}
							{...pending(g.is())}
							onclick={() => !g.is() && edit(index)}
						>
							<Icon name="pencil" />
						</button>
						<button
							class="icon-btn op"
							aria-label={m.meal_plan_delete_label({ time })}
							{...pending(g.is())}
							onclick={(e) => !g.is() && remove(index, e)}
						>
							<Icon name="trash" />
						</button>
					</div>
				</li>
			{/each}
		</ul>
		<p class="total">{m.meal_plan_total_portions_per_day({ total: m.feeder_portion({ count: perDay }) })}</p>
	</Loaded>
</div>

<Sheet
	open={editing !== null}
	onclose={() => (editing = null)}
	draft={editing?.draft}
	title={editing?.at === 'new' ? m.meal_plan_add_meal() : m.meal_plan_edit_meal()}
	description={m.feeder_meal_schedule_description()}
>
	{#if editing}
		<MealEditor draft={editing.draft} onsave={put} oncancel={() => (editing = null)} />
	{/if}
</Sheet>

<style>
	.meals {
		display: grid;
		gap: var(--s-3);
		container-type: inline-size;
	}
	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--s-3);
		flex-wrap: wrap;
	}
	.meal {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr);
		align-items: center;
		gap: var(--s-2) var(--s-3);
		min-height: var(--row-min);
		padding-block: var(--s-2);
		border-bottom: 1px solid var(--line);
	}
	.meal:last-child {
		border-bottom: 0;
	}
	.time {
		font: var(--t-group);
		font-variant-numeric: tabular-nums;
	}
	.what {
		display: grid;
		min-width: 0;
	}
	/* on/off, edit, delete: under the meal on a phone, at its end when there is room */
	.ops {
		grid-column: 1 / -1;
		display: flex;
		align-items: center;
		gap: var(--s-1);
	}
	.ops :global(.toggle) {
		margin-right: auto;
	}
	@container (min-width: 30rem) {
		.meal {
			grid-template-columns: auto minmax(0, 1fr) auto;
		}
		.ops {
			grid-column: auto;
		}
	}
	.op {
		width: var(--control-h);
		height: var(--control-h);
	}
	/* a meal switched off: said by its switch, dimmed only as an echo */
	.off .time,
	.off .what {
		color: var(--ink-muted);
	}
	.total {
		margin: 0;
		font: var(--t-secondary);
		color: var(--ink-muted);
	}
</style>
