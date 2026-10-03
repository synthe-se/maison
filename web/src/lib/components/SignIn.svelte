<script lang="ts">
	// Sign in with a passkey (Ariane's PasskeySignIn). No name to type (discoverable
	// credentials): one button, the passkey explained by what people know (fingerprint, face,
	// device code), and a word before the system dialog (FIDO Alliance design guidelines,
	// « handshake » messaging).
	import { m } from '#lib/paraglide/messages.js';
	import { supported } from '#lib/passkeys.ts';
	import { errorText } from '#lib/errors.ts';
	import { session } from '#lib/session.svelte.ts';
	import AuthCard from './AuthCard.svelte';
	import Icon from './Icon.svelte';
	import { pending } from '#lib/gesture.svelte.ts';

	let busy = $state(false);
	let error = $state('');
	const secure = supported();

	async function enter() {
		busy = true;
		error = '';
		try {
			await session.signIn();
		} catch (e) {
			error = errorText(e);
		} finally {
			busy = false;
		}
	}
</script>

<svelte:head><title>{m.pk_signin_title()} · {m.branding_name()}</title></svelte:head>

<AuthCard>
	<h1 tabindex="-1">{m.pk_signin_title()}</h1>
	{#if !secure}
		<p class="banner warn" role="alert">{m.pk_insecure()}</p>
	{:else}
		<p class="hint">{m.pk_signin_intro()}</p>
		<!-- a stable button: its label says the wait, it keeps the focus -->
		<button class="btn primary big" {...pending(busy)} onclick={() => !busy && enter()}>
			<Icon name="key" />{busy ? m.pk_waiting() : m.pk_button()}
		</button>
		<p class="hint">{m.pk_handshake()}</p>
		<p class="form-error" role="alert">{error}</p>
		<p class="hint aside">{m.pk_no_key()}</p>
	{/if}
</AuthCard>
