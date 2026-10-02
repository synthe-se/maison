<script lang="ts">
	// My account (Ariane's): my passkeys (FIDO « create, view and manage passkeys in account
	// settings »: one row per passkey, rename, remove, add), signing out everywhere, and for
	// admins, invitations. Every action goes through a Gesture; a refusal is worded by
	// passkeyMessage.
	import { tick } from 'svelte';
	import { m } from '#lib/paraglide/messages.js';
	import { date, when } from '#lib/i18n.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import {
		createInvite,
		listInvites,
		listPasskeys,
		register,
		removePasskey,
		renamePasskey,
		revokeInvite,
		worded,
		type CreatedInvite,
		type Invite,
		type PasskeyInfo
	} from '#lib/passkeys.ts';
	import { session } from '#lib/session.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import Toggle from '#lib/components/Toggle.svelte';

	const g = new Gesture();
	let keys = $state.raw<PasskeyInfo[]>([]);
	let invites = $state.raw<Invite[]>([]);
	let renaming = $state<string | null>(null);
	let newName = $state('');

	async function load() {
		keys = await listPasskeys().catch(() => keys);
		if (session.admin) invites = await listInvites().catch(() => invites);
	}
	$effect(() => void load());

	/** Sends, says `done`, reads the lists again. */
	const act = (send: () => Promise<unknown>, done?: () => string, key = '') =>
		g.run(() => worded(send()), async () => {
			if (done) ui.toast(done());
			await load();
		}, key);

	const keyName = (k: PasskeyInfo) => k.name || m.keys_unnamed();
	const longDate = (ms: number) => date(ms, { day: 'numeric', month: 'long', year: 'numeric' });

	async function rename(e: SubmitEvent, k: PasskeyInfo) {
		e.preventDefault();
		if ((await act(() => renamePasskey(k.id, newName), undefined, `rename:${k.id}`)) !== undefined) renaming = null;
	}

	// invitations: the form in a sheet; the link it makes shows on the page, to copy
	let inviting = $state(false);
	let inviteName = $state('');
	let inviteAdmin = $state(false);
	let created = $state.raw<(CreatedInvite & { name: string }) | null>(null);
	let opener = $state<HTMLButtonElement | null>(null);

	async function closeInvite() {
		inviting = false;
		await tick();
		opener?.focus();
	}

	async function invite(e: SubmitEvent) {
		e.preventDefault();
		const name = inviteName.trim();
		if (!name) return;
		const r = await act(() => createInvite(name, inviteAdmin), undefined, 'invite');
		if (!r) return;
		created = { ...(r as CreatedInvite), name };
		inviteName = '';
		inviteAdmin = false;
		await closeInvite();
	}

	async function copy() {
		if (!created) return;
		try {
			await navigator.clipboard.writeText(created.url);
			ui.toast(m.invites_copied());
		} catch {
			const field = document.getElementById('invite-url') as HTMLInputElement | null;
			field?.focus();
			field?.select();
		}
	}
</script>

<PageHead title={m.account_title()}>
	{#snippet sub()}{session.user?.name}{/snippet}
</PageHead>

<section class="block" aria-labelledby="keys-title">
	<h2 id="keys-title" class="group-title">{m.keys_title()}</h2>
	<p class="hint">{m.keys_intro()}</p>
	<ul class="list">
		{#each keys as k (k.id)}
			<li class="key-row">
				{#if renaming === k.id}
					<form class="inline" onsubmit={(e) => rename(e, k)}>
						<label class="sr-only" for="rename-{k.id}">{m.keys_rename_label()}</label>
						<!-- svelte-ignore a11y_autofocus -->
						<input id="rename-{k.id}" bind:value={newName} maxlength="60" autofocus />
						<button class="btn primary" disabled={!newName.trim() || g.is(`rename:${k.id}`)}>{m.common_save()}</button>
						<button class="btn ghost" type="button" onclick={() => (renaming = null)}>{m.common_cancel()}</button>
					</form>
				{:else}
					<span class="name"><span>{keyName(k)}</span>{#if k.backedUp}<span class="chip accent">{m.keys_synced()}</span>{/if}</span>
					<span class="facts">{m.keys_created({ when: longDate(k.createdMs) })} · {k.lastUsedMs ? m.keys_used({ when: when(k.lastUsedMs) }) : m.keys_never_used()}</span>
					<span class="acts">
						<button class="btn ghost" onclick={() => ((renaming = k.id), (newName = k.name))}>{m.common_rename()}</button>
						<button class="btn ghost" aria-label={m.keys_remove_label({ name: keyName(k) })} disabled={g.is(`remove:${k.id}`)} onclick={() => act(() => removePasskey(k.id), m.keys_removed, `remove:${k.id}`)}>{m.common_remove()}</button>
					</span>
				{/if}
			</li>
		{/each}
	</ul>
	<div>
		<button class="btn" disabled={g.is('add')} onclick={() => act(() => register(), m.keys_added, 'add')}>
			<Icon name="key" />{g.is('add') ? m.pk_waiting() : m.keys_add()}
		</button>
	</div>
</section>

<!-- a lost phone: every session ends, on every device; the passkeys stay -->
<section class="block" aria-labelledby="everywhere-title">
	<h2 id="everywhere-title" class="group-title">{m.everywhere_title()}</h2>
	<p class="hint">{m.everywhere_body()}</p>
	<div><button class="btn" onclick={() => g.run(() => session.signOutEverywhere(), undefined, 'everywhere')}>{m.everywhere_button()}</button></div>
</section>

{#if session.admin}
	<section class="block" aria-labelledby="invites-title">
		<h2 id="invites-title" class="group-title">{m.invites_title()}</h2>
		<p class="hint">{m.invites_intro()}</p>
		<div><button class="btn primary" aria-haspopup="dialog" bind:this={opener} onclick={() => (inviting = true)}>{m.invites_title()}</button></div>
		<Sheet open={inviting} title={m.invites_title()} onclose={closeInvite}>
			<form class="invite" onsubmit={invite}>
				<div class="field">
					<label for="invite-name">{m.invites_name()}</label>
					<!-- svelte-ignore a11y_autofocus -->
					<input id="invite-name" bind:value={inviteName} maxlength="60" autocomplete="off" autofocus />
				</div>
				<Toggle label={m.invites_admin()} checked={inviteAdmin} onchange={(on) => (inviteAdmin = on)} />
				<div class="actions">
					<button class="btn" type="button" onclick={closeInvite}>{m.common_cancel()}</button>
					<button class="btn primary" disabled={!inviteName.trim() || g.is('invite')}>{m.invites_create()}</button>
				</div>
			</form>
		</Sheet>
		{#if created}
			<div class="callout" role="group" aria-labelledby="created-title">
				<p id="created-title"><strong>{m.invites_link({ name: created.name })}</strong> · {m.invites_expires({ date: longDate(created.expiresMs) })}</p>
				<div class="inline">
					<label class="sr-only" for="invite-url">{m.invites_link({ name: created.name })}</label>
					<input id="invite-url" readonly value={created.url} onfocus={(e) => e.currentTarget.select()} />
					<button class="btn" onclick={copy}><Icon name="copy" />{m.invites_copy()}</button>
				</div>
			</div>
		{/if}
		<h3 class="label">{m.invites_pending()}</h3>
		{#if invites.length}
			<ul class="list">
				{#each invites as i (i.id)}
					<li class="key-row">
						<span class="name">{i.name}</span>
						<span class="facts">{m.invites_expires({ date: longDate(i.expiresMs) })}</span>
						<span class="acts">
							<button class="btn ghost" aria-label={m.invites_revoke_label({ name: i.name })} disabled={g.is(`revoke:${i.id}`)} onclick={() => act(() => revokeInvite(i.id), undefined, `revoke:${i.id}`)}>{m.invites_revoke()}</button>
						</span>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="hint">{m.invites_none()}</p>
		{/if}
	</section>
{/if}

<style>
	.block { display: grid; gap: var(--s-3); max-width: var(--measure); margin-bottom: var(--s-6); }
	.block p { margin: 0; }
	.label { font: var(--t-label); margin: var(--s-2) 0 0; }
	.key-row {
		display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--s-1) var(--s-3); align-items: center;
		min-height: var(--row-min); padding: var(--s-2) 0; border-bottom: 1px solid var(--line);
	}
	.name { font: var(--t-body); display: flex; gap: var(--s-2); align-items: center; flex-wrap: wrap; }
	.facts { grid-column: 1; font: var(--t-meta); color: var(--ink-muted); }
	.acts { grid-column: 2; grid-row: 1 / span 2; display: flex; gap: var(--s-1); flex-wrap: wrap; justify-content: end; }
	.inline { grid-column: 1 / -1; display: flex; gap: var(--s-2); flex-wrap: wrap; }
	.inline input { flex: 1 1 14rem; min-width: 0; }
	.invite { display: grid; gap: var(--s-4); }
	.actions { display: flex; gap: var(--s-2); justify-content: end; flex-wrap: wrap; }
</style>
