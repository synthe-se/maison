<script lang="ts">
	// Sign in with a passkey (Ariane's PasskeySignIn). No name to type (discoverable
	// credentials): one button, the passkey explained by what people know (fingerprint, face,
	// device code), and a word before the system dialog (FIDO Alliance design guidelines,
	// « handshake » messaging). Under it, the way for a wall screen: paired by an admin from a
	// code (ScreenPairing), no passkey on it.
	import { m } from '#lib/paraglide/messages.js';
	import { noPasskeys } from '#lib/passkeys.ts';
	import { pageTitle } from '#lib/i18n.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import AuthShell from './AuthShell.svelte';
	import Icon from './Icon.svelte';
	import ScreenPairing from './ScreenPairing.svelte';

	const g = new Gesture();
	const missing = noPasskeys();
	// the refusal is said under the button, the focus stays on it
	const enter = () => g.run(() => session.signIn(), undefined, 'signin', { inline: true });
	let pairing = $state(false);
</script>

<!-- the pairing says its own title -->
<svelte:head
	>{#if !pairing}<title>{pageTitle(m.pk_signin_title())}</title>{/if}</svelte:head
>

<AuthShell>
	{#if pairing}
		<ScreenPairing onback={() => (pairing = false)} />
	{:else}
		<section class="signin" aria-labelledby="signin-title">
			<h1 id="signin-title" tabindex="-1">{m.pk_signin_title()}</h1>
			{#if missing}
				<p class="banner warn" role="alert">{missing}</p>
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
			<div><button class="btn ghost" onclick={() => (pairing = true)}><Icon name="tv" />{m.screen_pair_button()}</button></div>
		</section>
	{/if}
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
