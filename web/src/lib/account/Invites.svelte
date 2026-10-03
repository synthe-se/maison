<script lang="ts">
	// Invitations (an admin's): the form in a sheet (a name, admin or not); the link it makes
	// shows on the page with the focus on it, ready to copy; the pending ones, each revoked
	// after asking (§ 6). A name already someone's (`person_exists`): their access back (a new
	// passkey for them), or another name.
	import { m } from '#lib/paraglide/messages.js';
	import { MAX_NAME, ApiError } from '#lib/api.ts';
	import { longDate } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import Toggle from '#lib/components/Toggle.svelte';
	import { createInvite, invitesData, revokeInvite, type CreatedInvite, type Invite } from '#lib/passkeys.ts';

	const invites = live(invitesData);
	const g = new Gesture();
	let title = $state<HTMLElement>();

	let inviting = $state(false);
	const EMPTY = { name: '', admin: false };
	const draft = new Draft({ ...EMPTY });
	const form = $derived(draft.current);
	let missing = $state('');
	/** The name is someone's already: give them their access back, or pick another name. */
	let existing = $state<{ id: string; name: string } | null>(null);
	let created = $state.raw<(CreatedInvite & { name: string }) | null>(null);
	let nameField = $state<HTMLInputElement>();

	function close() {
		inviting = false;
		missing = '';
		existing = null;
	}

	async function invite(person?: string) {
		const name = form.name.trim();
		missing = '';
		if (!name) {
			missing = m.invites_name_missing();
			return nameField?.focus();
		}
		const r = await g.run(() => createInvite(name, form.admin, person), undefined, 'invite', {
			field: () => nameField,
			// the server says who has the name: offered their access back, not an error
			refused: (e) => {
				if (!(e instanceof ApiError) || e.code !== 'person_exists') return false;
				existing = (e.detail?.person as { id: string; name: string } | undefined) ?? null;
				return true;
			}
		});
		if (!r) return;
		created = { ...r, name };
		draft.reset({ ...EMPTY });
		close();
		void invites.refresh();
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

	const revoke = (i: Invite) =>
		g.run(
			() => revokeInvite(i.id),
			async () => {
				ui.toast(m.invites_revoked({ name: i.name }));
				await invites.refresh();
				await refocus(title);
			},
			`revoke:${i.id}`
		);
</script>

<Group id="invites-title" title={m.invites_title()} bind:heading={title}>
	<p class="hint">{m.invites_intro()}</p>
	<div><button class="btn primary" aria-haspopup="dialog" onclick={() => (inviting = true)}>{m.invites_title()}</button></div>
	<Sheet open={inviting} title={m.invites_title()} onclose={close} {draft}>
		<form class="invite" onsubmit={(e) => (e.preventDefault(), void invite())} novalidate>
			<div class="field">
				<label for="invite-name">{m.invites_name()}</label>
				<input
					id="invite-name"
					bind:this={nameField}
					bind:value={form.name}
					oninput={() => (existing = null)}
					maxlength={MAX_NAME}
					autocomplete="off"
					aria-invalid={missing || g.error || existing ? 'true' : undefined}
					aria-describedby="invite-name-error"
				/>
				<p class="form-error" id="invite-name-error">{existing ? m.invites_person_exists({ name: existing.name }) : missing || g.error}</p>
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
			<Toggle label={m.invites_admin()} checked={form.admin} onchange={(on) => (form.admin = on)} />
			<div class="actions end">
				<button class="btn" type="button" onclick={close}>{m.common_cancel()}</button>
				{#if !existing}<button class="btn primary" {...pending(g.is('invite'))}>{m.invites_create()}</button>{/if}
			</div>
		</form>
	</Sheet>
	{#if created}
		<div class="callout" role="group" aria-labelledby="created-title">
			<p id="created-title">
				<strong>{m.invites_link({ name: created.name })}</strong> · {m.invites_expires({ date: longDate(created.expiresMs) })}
			</p>
			<div class="inline">
				<label class="sr-only" for="invite-url">{m.invites_link({ name: created.name })}</label>
				<input id="invite-url" readonly value={created.url} onfocus={(e) => e.currentTarget.select()} />
				<button class="btn" onclick={copy}><Icon name="copy" />{m.invites_copy()}</button>
			</div>
		</div>
	{/if}
	<h3 class="label sub">{m.invites_pending()}</h3>
	<Loaded value={invites} empty={invites.data?.length === 0} emptyText={m.invites_none()}>
		<ul class="plain-list">
			{#each invites.data ?? [] as i (i.id)}
				<ListRow second={m.invites_expires({ date: longDate(i.expiresMs) })}>
					{i.name}
					{#snippet end()}
						<ConfirmDialog
							ghost
							label={m.invites_revoke()}
							ariaLabel={m.invites_revoke_label({ name: i.name })}
							title={m.invites_revoke_title({ name: i.name })}
							description={m.invites_revoke_description()}
							action={m.invites_revoke()}
							busy={g.is(`revoke:${i.id}`)}
							onconfirm={() => revoke(i)}
						/>
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	</Loaded>
</Group>

<style>
	.sub {
		margin-top: var(--s-2);
	}
	.inline {
		display: flex;
		gap: var(--s-2);
		flex-wrap: wrap;
	}
	.inline input {
		flex: 1 1 14rem;
		min-width: 0;
	}
	.invite {
		display: grid;
		gap: var(--s-4);
	}
</style>
