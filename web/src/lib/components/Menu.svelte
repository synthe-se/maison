<script lang="ts" module>
	import type { IconName } from './Icon.svelte';

	/** One entry: an action (`onselect`), or a labelled switch that stays open (`oncheck`). */
	export type MenuItem =
		| { label: string; icon?: IconName; onselect: () => unknown }
		| { label: string; checked: boolean; oncheck: (on: boolean) => unknown };

	/** Items under an optional name; a line between two sections. */
	export interface MenuSection {
		heading?: string;
		items: MenuItem[];
	}
</script>

<script lang="ts">
	// The one menu (Bits UI DropdownMenu, APG menu button): an icon button named by `label` (busy
	// while its gesture travels), its sections of items. A switch item keeps the menu open and
	// says its state with a tick box, never color alone.
	import { DropdownMenu } from 'bits-ui';
	import { pending } from '#lib/gesture.svelte.ts';
	import Icon from './Icon.svelte';

	interface Props {
		label: string;
		icon?: IconName;
		/** The gesture of an item is in flight: the button says so, and keeps the focus. */
		busy?: boolean;
		sections: MenuSection[];
	}
	let { label, icon = 'ellipsis', busy = false, sections }: Props = $props();
	const shown = $derived(sections.filter((s) => s.items.length));
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger class="icon-btn" aria-label={label} {...pending(busy)}><Icon name={icon} {busy} /></DropdownMenu.Trigger>
	<DropdownMenu.Portal>
		<DropdownMenu.Content class="menu" align="end" sideOffset={4}>
			{#each shown as section, i (i)}
				{#if i > 0}<DropdownMenu.Separator class="menu-separator" />{/if}
				<DropdownMenu.Group>
					{#if section.heading}<DropdownMenu.GroupHeading class="menu-heading">{section.heading}</DropdownMenu.GroupHeading>{/if}
					{#each section.items as item (item.label)}
						{#if 'oncheck' in item}
							<DropdownMenu.CheckboxItem class="menu-item" checked={item.checked} closeOnSelect={false} onCheckedChange={item.oncheck}>
								{#snippet children({ checked })}
									<span class="menu-box" aria-hidden="true"
										>{#if checked}<Icon name="check" size={14} />{/if}</span
									>{item.label}
								{/snippet}
							</DropdownMenu.CheckboxItem>
						{:else}
							<DropdownMenu.Item class="menu-item" onSelect={item.onselect}
								>{#if item.icon}<Icon name={item.icon} />{/if}{item.label}</DropdownMenu.Item
							>
						{/if}
					{/each}
				</DropdownMenu.Group>
			{/each}
		</DropdownMenu.Content>
	</DropdownMenu.Portal>
</DropdownMenu.Root>

<style>
	/* the content is portalled: global, under the menu's own class */
	:global(.menu .menu-box) {
		display: inline-grid;
		place-items: center;
		width: var(--check-box);
		height: var(--check-box);
		flex: none;
		border: 1.5px solid var(--ink-muted);
		border-radius: var(--radius-s);
	}
</style>
