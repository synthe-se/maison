<script lang="ts">
	// My passkeys (FIDO « create, view and manage passkeys in account settings »): one row per
	// passkey, renamed in place, removed after asking (§ 6; the focus then goes to the list's
	// title), and « Ajouter une clé d’accès » from this device (asking the passkey again first
	// when the last sign-in is old: the server says `reauth_needed`).
	import { m } from '#lib/paraglide/messages.js';
	import { longDate, when } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import RenameField from '#lib/components/RenameField.svelte';
	import { passkeysData, register, removePasskey, renamePasskey, type PasskeyInfo } from '#lib/passkeys.ts';

	const keys = live(passkeysData);
	const g = new Gesture();
	let renaming = $state<string | null>(null);
	let title = $state<HTMLElement>();
	const keyName = (k: PasskeyInfo) => k.name || m.keys_unnamed();

	async function closeRename(k: PasskeyInfo) {
		renaming = null;
		// the row's Rename button comes back: the focus returns to it
		await refocus(() => document.getElementById(`rename-${k.id}`));
	}

	const add = () =>
		g.run(
			() => register({ onReauth: () => ui.say(m.keys_reauth()) }),
			async () => {
				ui.toast(m.keys_added());
				await keys.refresh();
			},
			'add'
		);

	const remove = (k: PasskeyInfo) =>
		g.run(
			() => removePasskey(k.id),
			async () => {
				ui.toast(m.keys_removed());
				await keys.refresh();
				await refocus(title);
			},
			`remove:${k.id}`
		);
</script>

<Group id="keys-title" title={m.keys_title()} bind:heading={title}>
	<p class="hint">{m.keys_intro()}</p>
	<Loaded value={keys}>
		<ul class="plain-list">
			{#each keys.data ?? [] as k (k.id)}
				{#snippet rename()}
					<RenameField
						label={m.keys_rename_label()}
						hideLabel
						autofocus
						value={k.name}
						save={(name) => renamePasskey(k.id, name).then(() => keys.refresh())}
						said={(name) => m.common_renamed({ name })}
						ondone={() => closeRename(k)}
					/>
				{/snippet}
				<ListRow
					whole={renaming === k.id ? rename : undefined}
					second="{m.keys_created({ when: longDate(k.createdMs) })} · {k.lastUsedMs
						? m.keys_used({ when: when(k.lastUsedMs) })
						: m.keys_never_used()}"
				>
					<span>{keyName(k)}</span>{#if k.backedUp}<span class="chip accent">{m.keys_synced()}</span>{/if}
					{#snippet end()}
						<button
							class="btn ghost"
							id="rename-{k.id}"
							aria-label={m.keys_rename_of({ name: keyName(k) })}
							onclick={() => (renaming = k.id)}>{m.common_rename()}</button
						>
						<ConfirmDialog
							ghost
							label={m.common_remove()}
							ariaLabel={m.keys_remove_label({ name: keyName(k) })}
							title={m.keys_remove_title({ name: keyName(k) })}
							description={m.keys_remove_description()}
							action={m.keys_remove_action()}
							busy={g.is(`remove:${k.id}`)}
							onconfirm={() => remove(k)}
						/>
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	</Loaded>
	<div>
		<!-- a stable button: its label says the wait, the focus stays on it -->
		<button class="btn" {...pending(g.is('add'))} onclick={add}>
			<Icon name="key" />{g.is('add') ? m.pk_waiting() : m.keys_add()}
		</button>
	</div>
</Group>
