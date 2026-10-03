<script lang="ts">
	// An invitation (Ariane's): greets the person by name, says what a passkey is in familiar
	// words, creates it, then confirms before going in (FIDO Alliance, « New account creation
	// with a passkey »: a step before the system dialog, a confirmation after). Also the way
	// back for someone who lost every passkey: an admin sends a new link.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { inviteGreeting, register, supported, type Registered } from '#lib/passkeys.ts';
	import { errorText } from '#lib/errors.ts';
	import { refocus } from '#lib/focus.ts';
	import { pending } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import AuthCard from '#lib/components/AuthCard.svelte';
	import Icon from '#lib/components/Icon.svelte';

	const token = $derived(page.params.token ?? '');
	let name = $state<string | null>(null);
	let error = $state('');
	let busy = $state(false);
	let created = $state.raw<Registered | null>(null);
	const secure = supported();
	let doneTitle = $state<HTMLElement>();

	$effect(() => {
		void inviteGreeting(token)
			.then((r) => (name = r.name))
			.catch((e) => (error = errorText(e)));
	});

	async function create() {
		busy = true;
		error = '';
		try {
			created = await register({ invite: token });
			// the button pressed is gone with the step: the focus goes to the new step's title
			void refocus(() => doneTitle);
		} catch (e) {
			error = errorText(e);
		} finally {
			busy = false;
		}
	}

	async function enter() {
		if (!created) return;
		session.adopt(created.user);
		await goto('/', { replaceState: true });
	}
</script>

<svelte:head><title>{name ? m.invite_title({ name }) : error ? m.invite_invalid_title() : m.invite_checking()} · {m.branding_name()}</title></svelte:head>

<AuthCard>
	{#if created}
		<p class="done" aria-hidden="true"><Icon name="check" size={28} /></p>
		<h1 tabindex="-1" bind:this={doneTitle}>{m.invite_done_title()}</h1>
		<p class="hint">{m.invite_done_body()}</p>
		<button class="btn primary big" onclick={enter}>{m.invite_enter()}</button>
	{:else if name}
		<h1 tabindex="-1">{m.invite_title({ name })}</h1>
		<p class="hint">{m.invite_intro()}</p>
		{#if !secure}
			<p class="banner warn" role="alert">{m.pk_insecure()}</p>
		{:else}
			<ul class="why">
				<li><Icon name="key" /><span>{m.invite_why_nopassword()}</span></li>
				<li><Icon name="copy" /><span>{m.invite_why_devices()}</span></li>
			</ul>
			<button class="btn primary big" {...pending(busy)} onclick={() => !busy && create()}>{busy ? m.pk_waiting() : m.invite_create()}</button>
			<p class="hint">{m.pk_handshake()}</p>
		{/if}
	{:else if error}
		<h1 tabindex="-1">{m.invite_invalid_title()}</h1>
	{:else}
		<h1 tabindex="-1">{m.invite_checking()}</h1>
	{/if}
	<p class="form-error" role="alert">{error}</p>
</AuthCard>

<style>
	.why { list-style: none; margin: 0; padding: 0; display: grid; gap: var(--s-3); }
	.why li { display: grid; grid-template-columns: 20px 1fr; gap: var(--s-3); align-items: start; color: var(--accent); }
	.why span { color: var(--ink); font: var(--t-body); }
	.done { display: inline-grid; place-items: center; width: 44px; height: 44px; border-radius: 50%; background: var(--accent-wash); color: var(--accent); }
</style>
