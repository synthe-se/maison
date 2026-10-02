<script lang="ts">
	// A determinate bar for an action longer than 10 s (docs/ux/tableau-de-bord.md § 2 and § 6,
	// NN/g): waking the TV from deep standby, sending an APK to the box.
	interface Props {
		label: string;
		value: number;
		max: number;
		/** The value in words (« 12 s sur 30 », « 40 % »). */
		valueText: string;
	}
	let { label, value, max, valueText }: Props = $props();
	const id = $props.id();
</script>

<div class="progress">
	<div class="head">
		<span id="{id}-label">{label}</span>
		<span class="value" aria-hidden="true">{valueText}</span>
	</div>
	<div
		class="track"
		role="progressbar"
		aria-labelledby="{id}-label"
		aria-valuemin={0}
		aria-valuemax={max}
		aria-valuenow={Math.round(value)}
		aria-valuetext={valueText}
	>
		<span class="fill" style:width="{Math.min(100, (value / max) * 100)}%"></span>
	</div>
</div>

<style>
	.progress { display: grid; gap: var(--s-1); }
	.head { display: flex; justify-content: space-between; gap: var(--s-3); font: var(--t-secondary); color: var(--ink-muted); }
	.value { font-variant-numeric: tabular-nums; }
	.track { height: 6px; border-radius: var(--radius-pill); background: var(--ground-raised); overflow: hidden; }
	.fill { display: block; height: 100%; background: var(--accent); transition: width 0.25s linear; }
	@media (prefers-reduced-motion: reduce) { .fill { transition: none; } }
</style>
