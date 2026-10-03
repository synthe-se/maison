<script lang="ts">
	// « Télécommande »: the infrared buttons of the AirTies set-top box (README « IR remote »).
	// The remote's picture and the list of configured buttons both open one editor, in a sheet.
	// Configuring is an admin's: a member sees what each button does.
	import { m } from '#lib/paraglide/messages.js';
	import { irApi, type IrBinding } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { session } from '#lib/session.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import RemoteMap from '#lib/devices/remote/RemoteMap.svelte';
	import BindingEditor from '#lib/devices/remote/BindingEditor.svelte';
	import { liveSources, summarize } from '#lib/devices/remote/actions.ts';
	import { keyName } from '#lib/devices/remote/keys.ts';

	const keymap = live('ir-keymap', irApi.keymap);
	const sources = liveSources();
	const map = $derived(keymap.data?.keymap ?? {});
	const bindings = $derived(
		Object.entries(map)
			.map(([code, b]): [number, IrBinding] => [Number(code), b])
			.sort((a, b) => a[0] - b[0])
	);

	/** The editor: closed, a new button (capture), or a key's binding. */
	let editing = $state<{ code?: number } | null>(null);
	let listTitle = $state<HTMLElement | null>(null);
	let editorDirty = $state(false);

	const g = new Gesture();
	const remove = (code: number) =>
		g.run(
			() => irApi.removeBinding(code),
			async () => {
				ui.say(m.remote_deleted({ key: keyName(code) }));
				await keymap.refresh();
				// the row and its button are gone: the focus goes to the list it was in
				await refocus(listTitle);
			},
			String(code)
		);
</script>

<PageHead title={m.remote_title()}>
	{#snippet sub()}{m.remote_subtitle()}{/snippet}
	{#snippet end()}
		<AdminOnly>
			<button class="btn primary" onclick={() => (editing = {})}><Icon name="plus" />{m.remote_add_binding()}</button>
		</AdminOnly>
	{/snippet}
</PageHead>

<Loaded value={keymap}>
	<div class="layout">
		{#if session.admin}
			<section class="group" aria-labelledby="remote-map-title">
				<h2 id="remote-map-title" class="group-title">{m.remote_map_title()}</h2>
				<RemoteMap keymap={map} onselect={(code) => (editing = { code })} />
			</section>
		{/if}

		<section class="group" aria-labelledby="remote-list-title">
			<div class="group-head">
				<h2 id="remote-list-title" class="group-title" tabindex="-1" bind:this={listTitle}>{m.remote_bindings_title()}</h2>
				{#if bindings.length}<span class="fact">{m.remote_binding_count({ count: bindings.length })}</span>{/if}
			</div>
			{#if bindings.length === 0}
				<div class="empty">
					<p>{m.remote_no_bindings()}</p>
					<p class="hint">{m.remote_no_bindings_hint()}</p>
				</div>
			{:else}
				<ul class="bindings">
					{#each bindings as [code, b] (code)}
						<li class="tile binding">
							<div class="tile-head">
								<span class="badge" aria-hidden="true">{keyName(code)}</span>
								<div class="text">
									<h3 class="tile-title">{b.label || keyName(code)}</h3>
									<p class="tile-state">
										{keyName(code)} · {m.remote_key_number({ code })}
										{#if b.repeat}<span class="chip"><Icon name="repeat" size={12} />{m.remote_repeat()}</span>{/if}
									</p>
								</div>
							</div>
							<ol class="summary">
								{#each b.actions as a, i (i)}<li>{summarize(a, sources.current)}</li>{/each}
							</ol>
							<AdminOnly reason={false}>
								<div class="actions end">
									<button class="btn" aria-label={m.remote_edit_binding({ key: keyName(code) })} onclick={() => (editing = { code })}>
										<Icon name="pen" />{m.common_edit()}
									</button>
									<ConfirmDialog
										label={m.remote_delete_key({ key: keyName(code) })}
										icon="trash"
										title={m.remote_delete_title({ key: keyName(code) })}
										description={m.remote_delete_consequence()}
										action={m.remote_delete_key({ key: keyName(code) })}
										pending={g.is(String(code))}
										onconfirm={() => remove(code)}
									/>
								</div>
							</AdminOnly>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
</Loaded>

<Sheet
	open={editing !== null}
	onclose={() => (editing = null)}
	dirty={editorDirty}
	title={editing?.code === undefined ? m.remote_new_key() : m.remote_edit_binding({ key: keyName(editing.code) })}
	description={m.remote_editor_description()}
>
	{#if editing}
		<BindingEditor
			code={editing.code}
			keymap={map}
			sources={sources.current}
			bind:dirty={editorDirty}
			onsaved={() => {
				editing = null;
				void keymap.refresh();
			}}
			oncancel={() => (editing = null)}
		/>
	{/if}
</Sheet>

<style>
	.layout { display: grid; gap: var(--s-6); align-items: start; }
	@media (min-width: 840px) {
		.layout { grid-template-columns: auto minmax(0, 1fr); }
	}
	.layout .group + .group { margin-top: 0; }
	.bindings { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-3); }
	.badge {
		flex: none; display: grid; place-items: center; min-width: var(--tile-icon); height: var(--tile-icon); padding: 0 var(--s-2);
		border-radius: var(--radius-m); background: var(--accent-wash); color: var(--accent); font: var(--t-label); font-weight: 600;
		max-width: 8rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}
	.summary { margin: 0; padding-left: var(--s-5); font: var(--t-secondary); color: var(--ink-muted); display: grid; gap: var(--s-1); }
	.summary li { overflow-wrap: anywhere; }
	.binding :global(.btn) { min-height: var(--control-h); }
</style>
