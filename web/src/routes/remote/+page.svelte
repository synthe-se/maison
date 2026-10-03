<script lang="ts">
	// « Télécommande »: the infrared buttons of the AirTies set-top box (AGENTS.md « IR remote »).
	// The remote's picture and the list of configured buttons both open one editor, in a sheet.
	// Configuring is an admin's: a member sees what each button does.
	import { m } from '#lib/paraglide/messages.js';
	import { irApi, type IrBinding } from '#lib/devices/remote/api.ts';
	import { keymap as keymapData } from '#lib/devices/remote/data.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { session } from '#lib/session.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import Group from '#lib/components/Group.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import AdminOnly from '#lib/components/AdminOnly.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import RemoteMap from '#lib/devices/remote/RemoteMap.svelte';
	import BindingEditor from '#lib/devices/remote/BindingEditor.svelte';
	import { bindingForm, liveSources, summarize, type BindingForm } from '#lib/devices/remote/actions.ts';
	import { keyName } from '#lib/devices/remote/keys.ts';

	const keymap = live(keymapData);
	const sources = liveSources();
	const map = $derived(keymap.data?.keymap ?? {});
	const bindings = $derived(
		Object.entries(map)
			.map(([code, b]): [number, IrBinding] => [Number(code), b])
			.toSorted((a, b) => a[0] - b[0])
	);

	/** The editor: closed, a new button (capture), or a key's binding, and its draft. */
	let editing = $state<{ code?: number; draft: Draft<BindingForm> } | null>(null);
	const edit = (code?: number) => (editing = { code, draft: new Draft(bindingForm(code === undefined ? undefined : map[String(code)])) });
	let listTitle = $state<HTMLElement>();

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
			<button class="btn primary" onclick={() => edit()}><Icon name="plus" />{m.remote_add_binding()}</button>
		</AdminOnly>
	{/snippet}
</PageHead>

<Loaded value={keymap}>
	<div class="layout">
		{#if session.admin}
			<Group id="remote-map-title" title={m.remote_map_title()}>
				<RemoteMap keymap={map} onselect={edit} />
			</Group>
		{/if}

		<Group
			id="remote-list-title"
			title={m.remote_bindings_title()}
			fact={bindings.length ? m.remote_binding_count({ count: bindings.length }) : undefined}
			bind:heading={listTitle}
		>
			{#if bindings.length === 0}
				<div class="empty">
					<p>{m.remote_no_bindings()}</p>
					<p class="hint">{m.remote_no_bindings_hint()}</p>
				</div>
			{:else}
				<ul class="bindings plain-list">
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
									<button class="btn" aria-label={m.remote_edit_binding({ key: keyName(code) })} onclick={() => edit(code)}>
										<Icon name="pen" />{m.common_edit()}
									</button>
									<ConfirmDialog
										label={m.remote_delete_key({ key: keyName(code) })}
										icon="trash"
										title={m.remote_delete_title({ key: keyName(code) })}
										description={m.remote_delete_consequence()}
										action={m.remote_delete_key({ key: keyName(code) })}
										busy={g.is(String(code))}
										onconfirm={() => remove(code)}
									/>
								</div>
							</AdminOnly>
						</li>
					{/each}
				</ul>
			{/if}
		</Group>
	</div>
</Loaded>

<Sheet
	open={editing !== null}
	onclose={() => (editing = null)}
	draft={editing?.draft}
	title={editing?.code === undefined ? m.remote_new_key() : m.remote_edit_binding({ key: keyName(editing.code) })}
	description={m.remote_editor_description()}
>
	{#if editing}
		<BindingEditor
			code={editing.code}
			keymap={map}
			sources={sources.current}
			draft={editing.draft}
			onsaved={() => {
				editing = null;
				void keymap.refresh();
			}}
			oncancel={() => (editing = null)}
		/>
	{/if}
</Sheet>

<style>
	.layout {
		display: grid;
		gap: var(--s-6);
		align-items: start;
	}
	@media (min-width: 840px) {
		.layout {
			grid-template-columns: auto minmax(0, 1fr);
		}
	}
	.layout :global(.group + .group) {
		margin-top: 0;
	}
	.bindings {
		display: grid;
		gap: var(--s-3);
	}
	.badge {
		flex: none;
		display: grid;
		place-items: center;
		min-width: var(--tile-icon);
		height: var(--tile-icon);
		padding: 0 var(--s-2);
		border-radius: var(--radius-m);
		background: var(--accent-wash);
		color: var(--accent);
		font: var(--t-label);
		font-weight: 600;
		max-width: 8rem;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.summary {
		margin: 0;
		padding-left: var(--s-5);
		font: var(--t-secondary);
		color: var(--ink-muted);
		display: grid;
		gap: var(--s-1);
	}
	.summary li {
		overflow-wrap: anywhere;
	}
	.binding :global(.btn) {
		min-height: var(--control-h);
	}
</style>
