<script lang="ts">
	// My account (Ariane's): my passkeys (PasskeyList), signing out everywhere, and for admins,
	// who may come in (People) and the invitations (Invites). What cannot be undone asks first
	// (§ 6); when the control pressed goes with its row, the focus goes to the section's title.
	import { m } from '#lib/paraglide/messages.js';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import PageHead from '#lib/components/PageHead.svelte';
	import Invites from '#lib/account/Invites.svelte';
	import PasskeyList from '#lib/account/PasskeyList.svelte';
	import People from '#lib/account/People.svelte';

	const admin = session.admin;
	const g = new Gesture();
</script>

<PageHead title={m.account_title()}>
	{#snippet sub()}{session.user?.name}{/snippet}
</PageHead>

<div class="sections">
	<PasskeyList />

	<!-- a lost phone: every session ends, on every device; the passkeys stay -->
	<Group id="everywhere-title" title={m.everywhere_title()}>
		<p class="hint">{m.everywhere_body()}</p>
		<div>
			<ConfirmDialog
				icon="log-out"
				label={m.everywhere_button()}
				title={m.everywhere_confirm_title()}
				description={m.everywhere_confirm_description()}
				action={m.everywhere_button()}
				busy={g.is()}
				onconfirm={() => g.run(() => session.signOutEverywhere())}
			/>
		</div>
	</Group>

	{#if admin}
		<People />
		<Invites />
	{/if}
</div>

<style>
	.sections {
		display: grid;
		gap: var(--s-6);
		max-width: var(--measure);
	}
	.sections :global(.group + .group) {
		margin-top: 0;
	}
	.sections :global(.group p) {
		margin: 0;
	}
</style>
