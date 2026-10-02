<script lang="ts">
	// Sign in with a passkey (Ariane's PasskeySignIn). No name to type (discoverable
	// credentials): one button, the passkey explained by what people know (fingerprint, face,
	// device code), and a word before the system dialog (FIDO Alliance design guidelines,
	// « handshake » messaging).
	import { m } from '#lib/paraglide/messages.js';
	import { passkeyMessage, supported } from '#lib/passkeys.ts';
	import { session } from '#lib/session.svelte.ts';
	import AuthCard from './AuthCard.svelte';
	import Icon from './Icon.svelte';

	let busy = $state(false);
	let error = $state('');
	const secure = supported();

	async function enter() {
		busy = true;
		error = '';
		try {
			await session.signIn();
		} catch (e) {
			error = passkeyMessage(e);
		} finally {
			busy = false;
		}
	}
</script>

<AuthCard>
	<h1 tabindex="-1">{m.pk_signin_title()}</h1>
	{#if !secure}
		<p class="banner warn" role="alert">{m.pk_insecure()}</p>
	{:else}
		<p class="hint">{m.pk_signin_intro()}</p>
		<button class="btn primary big" disabled={busy} onclick={enter}>
			<Icon name="key" />{busy ? m.pk_waiting() : m.pk_button()}
		</button>
		<p class="hint">{m.pk_handshake()}</p>
		<p class="form-error" role="alert">{error}</p>
		<p class="hint aside">{m.pk_no_key()}</p>
	{/if}
</AuthCard>
