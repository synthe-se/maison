<script lang="ts" generics="T extends string">
	// A text field that offers matches as one types (Bits UI Combobox, APG combobox with a
	// listbox popup: arrows go through the matches, Enter picks, Escape closes). The matches
	// come from the caller (a search on the server): it says where it is (`status`, the field's
	// description: searching, nothing found, a failure) and opens the list when they arrive.
	import { Combobox } from 'bits-ui';
	import Icon, { type IconName } from './Icon.svelte';

	interface Props {
		label: string;
		/** What is typed. */
		query: string;
		options: readonly { value: T; label: string }[];
		onpick: (value: T) => void;
		/** The list shown (opened when matches arrive). */
		open?: boolean;
		/** Ids of what describes the field (its section, its status line). */
		describedby?: string;
		invalid?: boolean;
		autocomplete?: HTMLInputElement['autocomplete'];
		/** Each match's icon. */
		icon?: IconName;
	}
	let {
		label,
		query = $bindable(),
		options,
		onpick,
		open = $bindable(false),
		describedby,
		invalid = false,
		autocomplete,
		icon
	}: Props = $props();
	const id = $props.id();
</script>

<div class="field">
	<label for="{id}-input">{label}</label>
	<Combobox.Root type="single" items={options.map((o) => ({ ...o }))} bind:open onValueChange={(v) => v && onpick(v as T)}>
		<Combobox.Input
			id="{id}-input"
			oninput={(e) => (query = e.currentTarget.value)}
			{autocomplete}
			aria-describedby={describedby}
			aria-invalid={invalid ? 'true' : undefined}
		/>
		<Combobox.Portal>
			<Combobox.Content class="select-content choice-content" sideOffset={4}>
				{#each options as o (o.value)}
					<Combobox.Item class="select-item" value={o.value} label={o.label}>
						{#if icon}<Icon name={icon} size={16} />{/if}{o.label}
					</Combobox.Item>
				{/each}
			</Combobox.Content>
		</Combobox.Portal>
	</Combobox.Root>
</div>
