<script lang="ts">
	// An invitation (Ariane's): greets the person by name, says what a passkey is in familiar
	// words, creates it, then confirms before going in (FIDO Alliance, « New account creation
	// with a passkey »: a step before the system dialog, a confirmation after). Also the way
	// back for someone who lost every passkey: an admin sends a new link.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import Door from '#lib/components/Door.svelte';
	import { inviteGreeting, register, noPasskeys, type Registered } from '#lib/passkeys.ts';
	import { errorText } from '#lib/errors.ts';
	import { refocus } from '#lib/focus.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import Icon from '#lib/components/Icon.svelte';

	const token = $derived(page.params.token ?? '');
	let name = $state<string | null>(null);
	/** The invitation could not be read (used, expired, or no answer). */
	let invalid = $state('');
	let created = $state.raw<Registered | null>(null);
	const missing = noPasskeys();
	let doneTitle = $state<HTMLElement>();
	const g = new Gesture();

	// the greeting for this token; an answer for an earlier one (the address changed) is dropped
	$effect(() => {
		let current = true;
		name = null;
		invalid = '';
		inviteGreeting(token).then(
			(r) => current && (name = r.name),
			(e) => current && (invalid = errorText(e))
		);
		return () => (current = false);
	});

	// the refusal said under the button, the focus staying on it
	const create = () =>
		g.run(
			() => register({ invite: token }),
			(r) => {
				created = r;
				// the button pressed is gone with the step: the focus goes to the new step's title
				void refocus(() => doneTitle);
			},
			'create',
			{ inline: true }
		);

	async function enter() {
		if (!created) return;
		session.adopt(created.user);
		await goto('/', { replaceState: true });
	}
</script>

{#snippet done()}<p class="done" aria-hidden="true"><Icon name="check" size={28} /></p>{/snippet}

<AuthShell>
	<Door
		title={created ? m.invite_done_title() : name ? m.invite_title({ name }) : invalid ? m.invite_invalid_title() : m.invite_checking()}
		mark={created ? done : undefined}
		bind:heading={doneTitle}
	>
		{#if created}
			<p class="lead">{m.invite_done_body()}</p>
			<button class="btn primary big" onclick={enter}>{m.invite_enter()}</button>
		{:else if name}
			<p class="lead">{m.invite_intro()}</p>
			{#if missing}
				<p class="banner warn" role="alert">{missing}</p>
			{:else}
				<ul class="why plain-list">
					<li><Icon name="key" /><span>{m.invite_why_nopassword()}</span></li>
					<li><Icon name="copy" /><span>{m.invite_why_devices()}</span></li>
				</ul>
				<button class="btn primary big" {...pending(g.is())} onclick={create}>{g.is() ? m.pk_waiting() : m.invite_create()}</button>
				<p class="hint">{m.pk_handshake()}</p>
			{/if}
		{/if}
		<p class="form-error" role="alert">{invalid || g.error}</p>
	</Door>
</AuthShell>

<style>
	.why {
		display: grid;
		gap: var(--s-3);
	}
	.why li {
		display: grid;
		grid-template-columns: var(--lead) 1fr;
		gap: var(--s-3);
		align-items: start;
		color: var(--accent);
	}
	.why span {
		color: var(--ink);
		font: var(--t-body);
	}
	.done {
		display: inline-grid;
		place-items: center;
		width: var(--control-h);
		height: var(--control-h);
		border-radius: 50%;
		background: var(--accent-wash);
		color: var(--accent);
	}
</style>
