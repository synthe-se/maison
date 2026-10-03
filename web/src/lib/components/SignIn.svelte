<script lang="ts">
	// Sign in with a passkey (Ariane's PasskeySignIn). No name to type (discoverable
	// credentials): one button, the passkey explained by what people know (fingerprint, face,
	// device code), and a word before the system dialog (FIDO Alliance design guidelines,
	// « handshake » messaging).
	import { m } from '#lib/paraglide/messages.js';
	import { supported } from '#lib/passkeys.ts';
	import { pageTitle } from '#lib/i18n.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import AuthShell from './AuthShell.svelte';
	import Icon from './Icon.svelte';

	const g = new Gesture();
	const secure = supported();
	// the refusal is said under the button, the focus stays on it
	const enter = () => g.run(() => session.signIn(), undefined, 'signin', { inline: true });
</script>

<svelte:head><title>{pageTitle(m.pk_signin_title())}</title></svelte:head>

<AuthShell>
	<section class="signin" aria-labelledby="signin-title">
		<h1 id="signin-title" tabindex="-1">{m.pk_signin_title()}</h1>
		{#if !secure}
			<p class="banner warn" role="alert">{m.pk_insecure()}</p>
		{:else}
			<p class="lead">{m.pk_signin_intro()}</p>
			<!-- a stable button: its label says the wait, it keeps the focus -->
			<button class="btn primary big" {...pending(g.is())} onclick={enter}>
				<Icon name="key" />{g.is() ? m.pk_waiting() : m.pk_button()}
			</button>
			<p class="hint">{m.pk_handshake()}</p>
			<p class="form-error" role="alert">{g.error}</p>
			<p class="hint aside">{m.pk_no_key()}</p>
		{/if}
	</section>
</AuthShell>

<style>
	.signin {
		display: grid;
		gap: var(--s-4);
	}
	.signin p {
		margin: 0;
	}
</style>
