<script lang="ts">
	// The remote as it is in the hand: every key opens its binding. A configured key is filled,
	// a free one is an empty outline, and its name says which (not the colour alone).
	import { m } from '#lib/paraglide/messages.js';
	import type { IrBinding } from '#lib/api.ts';
	import Icon from '#lib/components/Icon.svelte';
	import { REMOTE_ROWS } from './keys.ts';

	interface Props {
		keymap: Record<string, IrBinding>;
		onselect: (code: number) => void;
	}
	let { keymap, onselect }: Props = $props();
</script>

<div class="remote">
	{#each REMOTE_ROWS as row, i (i)}
		<div class="row" class:end={i === 0}>
			{#each row as key (key.code)}
				{@const binding = keymap[String(key.code)]}
				<button
					type="button"
					class="key"
					class:mapped={binding}
					aria-label={binding
						? m.remote_key_mapped({ key: key.name(), label: binding.label || key.name() })
						: m.remote_key_free({ key: key.name() })}
					onclick={() => onselect(key.code)}
				>
					{#if key.icon}<Icon name={key.icon} size={18} />{:else}{key.glyph}{/if}
				</button>
			{/each}
		</div>
	{/each}
	<p class="legend hint" aria-hidden="true">
		<span><span class="dot mapped"></span>{m.remote_legend_mapped()}</span>
		<span><span class="dot"></span>{m.remote_legend_free()}</span>
	</p>
	<p class="hint">{m.remote_visual_hint()}</p>
</div>

<style>
	.remote {
		display: grid; gap: var(--s-2); justify-items: center; width: fit-content; margin-inline: auto;
		padding: var(--s-4); border: 1px solid var(--line); border-radius: var(--radius-2xl); background: var(--surface);
	}
	.row { display: flex; gap: var(--s-2); justify-content: center; width: 100%; }
	.row.end { justify-content: flex-end; }
	.key {
		display: grid; place-items: center; min-width: var(--control-h); height: var(--control-h); padding: 0 var(--s-2);
		border-radius: var(--radius-pill); border: 1px solid var(--line); background: var(--ground); color: var(--ink-muted);
		font: var(--t-label); cursor: pointer; font-variant-numeric: tabular-nums;
	}
	.key:hover { border-color: var(--accent-soft); color: var(--ink); }
	.key.mapped { background: var(--accent); border-color: var(--accent); color: var(--on-accent); }
	.legend { display: flex; gap: var(--s-4); justify-content: center; }
	.legend > span { display: inline-flex; align-items: center; gap: var(--s-1); }
	.dot { width: 12px; height: 12px; border-radius: 50%; border: 1px solid var(--ink-muted); }
	.dot.mapped { background: var(--accent); border-color: var(--accent); }
	.hint { text-align: center; }
</style>
