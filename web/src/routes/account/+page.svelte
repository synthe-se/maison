<script lang="ts">
	// My account (Ariane's): my passkeys (FIDO « create, view and manage passkeys in account
	// settings »: one row per passkey, rename, remove, add), signing out everywhere, and for
	// admins, who may come in and the invitations. What cannot be undone asks first (§ 6); when
	// the control pressed goes with its row, the focus goes to the section's title.
	import { m } from '#lib/paraglide/messages.js';
	import { date, when } from '#lib/i18n.svelte.ts';
	import { ApiError, MAX_NAME, peopleApi, type Person } from '#lib/api.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { createInvite, listInvites, listPasskeys, register, removePasskey, renamePasskey, revokeInvite, type CreatedInvite, type PasskeyInfo } from '#lib/passkeys.ts';
	import { session } from '#lib/session.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import Icon from '#lib/components/Icon.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import RenameField from '#lib/components/RenameField.svelte';
	import Loaded from '#lib/components/Loaded.svelte';

	const admin = session.admin;
	const keys = live('passkeys', listPasskeys);
	const invites = admin ? live('invites', listInvites) : undefined;
	const people = admin ? live('people', peopleApi.list) : undefined;

	const g = new Gesture();
	let renaming = $state<string | null>(null);
	let keysTitle = $state<HTMLElement>();
	let peopleTitle = $state<HTMLElement>();
	let invitesTitle = $state<HTMLElement>();

	const keyName = (k: PasskeyInfo) => k.name || m.keys_unnamed();
	const longDate = (ms: number) => date(ms, { day: 'numeric', month: 'long', year: 'numeric' });

	/** Sends; on success says `done`, reads `list` again and puts the focus on `then`. */
	const act = (key: string, send: () => Promise<unknown>, done: string | undefined, list: { refresh: () => Promise<void> } | undefined, then?: () => HTMLElement | undefined) =>
		g.run(
			send,
			async () => {
				if (done) ui.toast(done);
				await list?.refresh();
				if (then) await refocus(then());
			},
			key
		);

	async function closeRename(k: PasskeyInfo) {
		renaming = null;
		// the row's Rename button comes back: the focus returns to it
		await refocus(`#rename-${CSS.escape(k.id)}`);
	}

	const addKey = () =>
		act('add', () => register({ onReauth: () => ui.say(m.keys_reauth()) }), m.keys_added(), keys);

	// ── invitations: the form in a sheet; the link it makes shows on the page, to copy ──
	let inviting = $state(false);
	let inviteName = $state('');
	let inviteAdmin = $state(false);
	let inviteError = $state('');
	/** The name is someone's already: give them their access back, or pick another name. */
	let existing = $state<Person | { id: string; name: string } | null>(null);
	let created = $state.raw<(CreatedInvite & { name: string }) | null>(null);
	let opener = $state<HTMLButtonElement>();
	let nameField = $state<HTMLInputElement>();
	const dirty = $derived(inviteName.trim() !== '' || inviteAdmin);

	async function closeInvite() {
		inviting = false;
		inviteError = '';
		existing = null;
		await refocus(opener);
	}

	async function invite(person?: string) {
		const name = inviteName.trim();
		inviteError = '';
		if (!name) {
			inviteError = m.invites_name_missing();
			return nameField?.focus();
		}
		let refusal: unknown;
		const r = await g.run(
			() =>
				createInvite(name, inviteAdmin, person).catch((err) => {
					refusal = err;
					throw err;
				}),
			undefined,
			'invite',
			{ field: () => nameField }
		);
		if (!r) {
			if (refusal instanceof ApiError && refusal.code === 'person_exists') {
				g.error = '';
				// the server says who has the name
				existing = (refusal.detail?.person as { id: string; name: string } | undefined) ?? null;
			}
			return;
		}
		created = { ...r, name };
		inviteName = '';
		inviteAdmin = false;
		existing = null;
		inviting = false;
		void invites?.refresh();
		// the link is the outcome: said, and the focus on it, ready to copy
		ui.say(m.invites_link_ready({ name }));
		await refocus('#invite-url');
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

<section class="block section" aria-labelledby="keys-title">
	<h2 id="keys-title" class="group-title" tabindex="-1" bind:this={keysTitle}>{m.keys_title()}</h2>
	<p class="hint">{m.keys_intro()}</p>
	<Loaded value={keys}>
		<ul class="list">
			{#each keys.data ?? [] as k (k.id)}
				<li class="key-row">
					{#if renaming === k.id}
						<div class="whole">
							<RenameField
								label={m.keys_rename_label()}
								hideLabel
								autofocus
								value={k.name}
								save={(name) => renamePasskey(k.id, name).then(() => keys.refresh())}
								said={(name) => m.common_renamed({ name })}
								ondone={() => closeRename(k)}
							/>
						</div>
					{:else}
						<span class="name"><span>{keyName(k)}</span>{#if k.backedUp}<span class="chip accent">{m.keys_synced()}</span>{/if}</span>
						<span class="facts">{m.keys_created({ when: longDate(k.createdMs) })} · {k.lastUsedMs ? m.keys_used({ when: when(k.lastUsedMs) }) : m.keys_never_used()}</span>
						<span class="acts">
							<button class="btn ghost" id="rename-{k.id}" aria-label={m.keys_rename_of({ name: keyName(k) })} onclick={() => (renaming = k.id)}>{m.common_rename()}</button>
							<ConfirmDialog
								ghost
								label={m.common_remove()}
								ariaLabel={m.keys_remove_label({ name: keyName(k) })}
								title={m.keys_remove_title({ name: keyName(k) })}
								description={m.keys_remove_description()}
								action={m.keys_remove_action()}
								pending={g.is(`remove:${k.id}`)}
								onconfirm={() => act(`remove:${k.id}`, () => removePasskey(k.id), m.keys_removed(), keys, () => keysTitle)}
							/>
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	</Loaded>
	<div>
		<!-- a stable button: its label says the wait, the focus stays on it -->
		<button class="btn" {...pending(g.is('add'))} onclick={addKey}>
			<Icon name="key" />{g.is('add') ? m.pk_waiting() : m.keys_add()}
		</button>
	</div>
</section>

<!-- a lost phone: every session ends, on every device; the passkeys stay -->
<section class="block section" aria-labelledby="everywhere-title">
	<h2 id="everywhere-title" class="group-title">{m.everywhere_title()}</h2>
	<p class="hint">{m.everywhere_body()}</p>
	<div>
		<ConfirmDialog
			icon="log-out"
			label={m.everywhere_button()}
			title={m.everywhere_confirm_title()}
			description={m.everywhere_confirm_description()}
			action={m.everywhere_button()}
			pending={g.is('everywhere')}
			onconfirm={() => g.run(() => session.signOutEverywhere(), undefined, 'everywhere')}
		/>
	</div>
</section>

{#if admin && people && invites}
	<section class="block section" aria-labelledby="people-title">
		<h2 id="people-title" class="group-title" tabindex="-1" bind:this={peopleTitle}>{m.people_title()}</h2>
		<p class="hint">{m.people_intro()}</p>
		<Loaded value={people}>
			<ul class="list">
				{#each people.data ?? [] as p (p.id)}
					<li class="key-row">
						<span class="name"><span>{p.name}</span>{#if p.role === 'admin'}<span class="chip">{m.people_admin()}</span>{/if}</span>
						<span class="facts">{m.people_passkeys({ count: p.passkeys })}</span>
						<span class="acts">
							{#if p.id === session.user?.id}
								<span class="hint">{m.people_you()}</span>
							{:else}
								<ConfirmDialog
									ghost
									danger
									label={m.common_remove()}
									ariaLabel={m.common_remove_named({ name: p.name })}
									title={m.common_remove_from_maison({ name: p.name })}
									description={m.people_remove_description()}
									action={m.people_remove_action()}
									pending={g.is(`person:${p.id}`)}
									onconfirm={() => act(`person:${p.id}`, () => peopleApi.remove(p.id), m.people_removed({ name: p.name }), people, () => peopleTitle)}
								/>
							{/if}
						</span>
					</li>
				{/each}
			</ul>
		</Loaded>
	</section>

	<section class="block section" aria-labelledby="invites-title">
		<h2 id="invites-title" class="group-title" tabindex="-1" bind:this={invitesTitle}>{m.invites_title()}</h2>
		<p class="hint">{m.invites_intro()}</p>
		<div><button class="btn primary" aria-haspopup="dialog" bind:this={opener} onclick={() => (inviting = true)}>{m.invites_title()}</button></div>
		<Sheet open={inviting} title={m.invites_title()} onclose={closeInvite} {dirty}>
			<form class="invite" onsubmit={(e) => (e.preventDefault(), void invite())} novalidate>
				<div class="field">
					<label for="invite-name">{m.invites_name()}</label>
					<input
						id="invite-name"
						bind:this={nameField}
						bind:value={inviteName}
						oninput={() => (existing = null)}
						maxlength={MAX_NAME}
						autocomplete="off"
						aria-invalid={inviteError || g.error || existing ? 'true' : undefined}
						aria-describedby="invite-name-error"
					/>
					<p class="form-error" id="invite-name-error">{existing ? m.invites_person_exists({ name: existing.name }) : inviteError || g.error}</p>
				</div>
				{#if existing}
					{@const who = existing}
					<!-- the name is someone's: their access back (a new passkey for them), or another name -->
					<div class="actions">
						<button class="btn primary" type="button" {...pending(g.is('invite'))} onclick={() => invite(who.id)}>
							{m.invites_access_back({ name: who.name })}
						</button>
						<button class="btn" type="button" onclick={() => ((existing = null), nameField?.select())}>{m.invites_other_name()}</button>
					</div>
				{/if}
				<Toggle label={m.invites_admin()} checked={inviteAdmin} onchange={(on) => (inviteAdmin = on)} />
				<div class="actions end">
					<button class="btn" type="button" onclick={closeInvite}>{m.common_cancel()}</button>
					{#if !existing}<button class="btn primary" {...pending(g.is('invite'))}>{m.invites_create()}</button>{/if}
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
		<h3 class="label sub">{m.invites_pending()}</h3>
		<Loaded value={invites} empty={invites.data?.length === 0} emptyText={m.invites_none()}>
			<ul class="list">
				{#each invites.data ?? [] as i (i.id)}
					<li class="key-row">
						<span class="name">{i.name}</span>
						<span class="facts">{m.invites_expires({ date: longDate(i.expiresMs) })}</span>
						<span class="acts">
							<ConfirmDialog
								ghost
								label={m.invites_revoke()}
								ariaLabel={m.invites_revoke_label({ name: i.name })}
								title={m.invites_revoke_title({ name: i.name })}
								description={m.invites_revoke_description()}
								action={m.invites_revoke()}
								pending={g.is(`revoke:${i.id}`)}
								onconfirm={() => act(`revoke:${i.id}`, () => revokeInvite(i.id), m.invites_revoked({ name: i.name }), invites, () => invitesTitle)}
							/>
						</span>
					</li>
				{/each}
			</ul>
		</Loaded>
	</section>
{/if}

<style>
	.section { max-width: var(--measure); margin-bottom: var(--s-6); gap: var(--s-3); }
	.section p { margin: 0; }
	.sub { margin-top: var(--s-2); }
	.key-row {
		display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: var(--s-1) var(--s-3); align-items: center;
		min-height: var(--row-min); padding: var(--s-2) 0; border-bottom: 1px solid var(--line);
	}
	.name { font: var(--t-body); display: flex; gap: var(--s-2); align-items: center; flex-wrap: wrap; }
	.facts { grid-column: 1; font: var(--t-meta); color: var(--ink-muted); }
	.acts { grid-column: 2; grid-row: 1 / span 2; display: flex; gap: var(--s-1); flex-wrap: wrap; justify-content: end; align-items: center; }
	.whole { grid-column: 1 / -1; }
	.inline { display: flex; gap: var(--s-2); flex-wrap: wrap; }
	.inline input { flex: 1 1 14rem; min-width: 0; }
	.invite { display: grid; gap: var(--s-4); }
</style>
