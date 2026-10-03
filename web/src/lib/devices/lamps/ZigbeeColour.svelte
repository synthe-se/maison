<script lang="ts">
	// Colour for a Zigbee lamp: named swatches (keyboard, screen readers), the wheel for a
	// pointer, and the lamp's built-in effects.
	import { m } from '#lib/paraglide/messages.js';
	import { zigbeeLampsApi, type ZigbeeLamp } from '#lib/api.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { css, EFFECTS, paintWheel, PRESETS, rgbToXy, STOP_EFFECT, wheelColor, wheelPosition } from './color.ts';
	import { zigbee } from './lamp.ts';

	let { lamp, disabled }: { lamp: ZigbeeLamp; disabled: boolean } = $props();

	const g = new Gesture();
	let canvas = $state<HTMLCanvasElement | null>(null);
	$effect(() => {
		if (canvas) paintWheel(canvas);
	});
	const marker = $derived(
		lamp.state.colorX !== null && lamp.state.colorY !== null ? wheelPosition(lamp.state.colorX, lamp.state.colorY) : null
	);

	// one order at a time: a swatch or an effect pressed while one travels does nothing (busy, not disabled: the focus stays)
	const act = (run: () => Promise<unknown>) => g.run(run, () => refresh(zigbee.key));

	const setColour = (xy: { x: number; y: number }) => act(() => zigbeeLampsApi.color(lamp.id, xy.x, xy.y));

	function pick(e: MouseEvent) {
		if (disabled || g.is() || !canvas) return;
		const box = canvas.getBoundingClientRect();
		const xy = wheelColor((e.clientX - box.left) / box.width, (e.clientY - box.top) / box.height);
		if (xy) void setColour(xy);
	}
</script>

<div class="colour">
	<div class="swatches" role="group" aria-label={m.zigbee_lamps_color()}>
		{#each PRESETS as p (p.rgb.join())}
			<button
				class="swatch"
				style:background={css(p.rgb)}
				aria-label={p.name()}
				title={p.name()}
				{disabled} {...pending(g.is())}
				onclick={() => setColour(rgbToXy(p.rgb))}
			></button>
		{/each}
	</div>

	<!-- pointer-only shortcut to any colour; the swatches above are the accessible way -->
	<div class="wheel" class:off={disabled} aria-hidden="true">
		<canvas bind:this={canvas} width="192" height="192" onclick={pick}></canvas>
		{#if marker}<span class="marker" style:left="{marker.left * 100}%" style:top="{marker.top * 100}%"></span>{/if}
	</div>

	<section aria-labelledby="zigbee-effects">
		<h3 id="zigbee-effects" class="label">{m.zigbee_lamps_effects()}</h3>
		<div class="actions">
			{#each EFFECTS as eff (eff.id)}
				<button class="btn" {disabled} {...pending(g.is())} onclick={() => act(() => zigbeeLampsApi.effect(lamp.id, eff.id))}>{eff.name()}</button>
			{/each}
			<button class="btn ghost" {disabled} {...pending(g.is())} onclick={() => act(() => zigbeeLampsApi.effect(lamp.id, STOP_EFFECT))}>
				{m.zigbee_lamps_effect_stop_effect()}
			</button>
		</div>
	</section>
</div>

<style>
	.colour { display: grid; gap: var(--s-4); }
	.swatches { display: flex; flex-wrap: wrap; gap: var(--s-2); }
	/* 44 px targets, a ring so white reads on a light ground */
	.swatch { width: var(--control-h); height: var(--control-h); border-radius: 50%; border: 2px solid var(--line); cursor: pointer; padding: 0; }
	.swatch:disabled { opacity: 0.4; cursor: not-allowed; }
	.wheel { position: relative; width: 192px; height: 192px; }
	.wheel canvas { width: 100%; height: 100%; border-radius: 50%; cursor: crosshair; display: block; }
	.wheel.off { opacity: 0.4; pointer-events: none; }
	.marker {
		position: absolute; width: 16px; height: 16px; transform: translate(-50%, -50%); border-radius: 50%;
		border: 2px solid var(--surface); box-shadow: var(--shadow); pointer-events: none;
	}
	section { display: grid; gap: var(--s-2); }
</style>
