<script lang="ts">
	// One labelled text field of a form, said once: its label, the input (any of its attributes
	// passed through), and what is wrong with it under it, the input described by it and marked
	// invalid while it says something (WCAG 3.3.1). The focus is the form's to give (`input`).
	import type { HTMLInputAttributes } from 'svelte/elements';

	interface Props extends Omit<HTMLInputAttributes, 'value' | 'id'> {
		label: string;
		value: string;
		/** What is wrong, under the field; '' when nothing. */
		error?: string;
		/** The input, to give it the focus. */
		input?: HTMLInputElement;
		id?: string;
	}
	let { label, value = $bindable(), error = '', input = $bindable(), id: given, ...rest }: Props = $props();
	const uid = $props.id();
	const id = $derived(given ?? `${uid}-field`);
</script>

<div class="field">
	<label for={id}>{label}</label>
	<input {...rest} {id} bind:this={input} bind:value aria-invalid={error ? 'true' : undefined} aria-describedby="{id}-error" />
	<p class="form-error" id="{id}-error">{error}</p>
</div>
