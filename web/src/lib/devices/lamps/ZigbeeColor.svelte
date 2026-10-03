<script lang="ts">
	// Color for a Zigbee lamp: named swatches (keyboard, screen readers), the wheel for a
	// pointer, and the lamp's built-in effects. A lamp that is off or out of reach keeps them in
	// place, not operable, saying why above them.
	import { m } from '#lib/paraglide/messages.js';
	import { refresh } from '#lib/live.svelte.ts';
	import { Gesture, pending, unavailable } from '#lib/gesture.svelte.ts';
	import { zigbeeLampsApi, type ZigbeeLamp } from './api.ts';
	import { css, EFFECTS, paintWheel, PRESETS, rgbToXy, STOP_EFFECT, wheelColor, wheelPosition } from './color.ts';
	import { fromZigbee, lampState, zigbee } from './lamp.ts';

	let { lamp }: { lamp: ZigbeeLamp } = $props();
	const id = $props.id();

	const g = new Gesture();
	const why = $derived(!lamp.reachable ? lampState(fromZigbee(lamp)) : !lamp.state.isOn ? m.lamps_off_reason() : '');
	/** What each control spreads: unavailable with its reason, else busy while an order travels. */
	const control = $derived(why ? unavailable(`${id}-why`) : pending(g.is()));
	let canvas = $state<HTMLCanvasElement | null>(null);
	$effect(() => {
		if (canvas) paintWheel(canvas);
	});
	const marker = $derived(
		lamp.state.colorX !== null && lamp.state.colorY !== null ? wheelPosition(lamp.state.colorX, lamp.state.colorY) : null
	);

	// one order at a time: a swatch or an effect pressed while one travels does nothing (busy,
	// not disabled: the focus stays)
	const act = (run: () => Promise<unknown>) => !why && g.run(run, () => refresh(zigbee.key));

	const setColor = (xy: { x: number; y: number }) => act(() => zigbeeLampsApi.color(lamp.id, xy.x, xy.y));

	function pick(e: MouseEvent) {
		if (why || g.is() || !canvas) return;
		const box = canvas.getBoundingClientRect();
		const xy = wheelColor((e.clientX - box.left) / box.width, (e.clientY - box.top) / box.height);
		if (xy) void setColor(xy);
	}
</script>

<div class="color">
	{#if why}<p class="hint" id="{id}-why">{why}</p>{/if}
	<div class="swatches" role="group" aria-label={m.zigbee_lamps_color()}>
		{#each PRESETS as p (p.rgb.join())}
			<button
				class="swatch"
				style:background={css(p.rgb)}
				aria-label={p.name()}
				title={p.name()}
				{...control}
				onclick={() => setColor(rgbToXy(p.rgb))}
			></button>
		{/each}
	</div>

	<!-- pointer-only shortcut to any color; the swatches above are the accessible way -->
	<div class="wheel" class:off={!!why} aria-hidden="true">
		<canvas bind:this={canvas} width="192" height="192" onclick={pick}></canvas>
		{#if marker}<span class="marker" style:left="{marker.left * 100}%" style:top="{marker.top * 100}%"></span>{/if}
	</div>

	<section aria-labelledby="{id}-effects">
		<h3 id="{id}-effects" class="label">{m.zigbee_lamps_effects()}</h3>
		<div class="actions">
			{#each EFFECTS as eff (eff.id)}
				<button class="btn" {...control} onclick={() => act(() => zigbeeLampsApi.effect(lamp.id, eff.id))}>{eff.name()}</button>
			{/each}
			<button class="btn ghost" {...control} onclick={() => act(() => zigbeeLampsApi.effect(lamp.id, STOP_EFFECT))}>
				{m.zigbee_lamps_effect_stop_effect()}
			</button>
		</div>
	</section>
</div>

<style>
	.color {
		display: grid;
		gap: var(--s-4);
	}
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: var(--s-2);
	}
	/* 44 px targets, a ring so white reads on a light ground */
	.swatch {
		width: var(--control-h);
		height: var(--control-h);
		border-radius: 50%;
		border: 2px solid var(--line);
		cursor: pointer;
		padding: 0;
	}
	.swatch[aria-disabled='true'] {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}
	.wheel {
		position: relative;
		width: var(--wheel);
		height: var(--wheel);
	}
	.wheel canvas {
		width: 100%;
		height: 100%;
		border-radius: 50%;
		cursor: crosshair;
		display: block;
	}
	.wheel.off {
		opacity: var(--disabled-opacity);
		pointer-events: none;
	}
	.marker {
		position: absolute;
		width: var(--marker);
		height: var(--marker);
		transform: translate(-50%, -50%);
		border-radius: 50%;
		border: 2px solid var(--surface);
		box-shadow: var(--shadow);
		pointer-events: none;
	}
	section {
		display: grid;
		gap: var(--s-2);
	}
</style>
