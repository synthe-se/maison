<script lang="ts">
	import '@fontsource-variable/fraunces';
	import '../styles/app.css';
	import { afterNavigate, beforeNavigate } from '$app/navigation';
	import { page, updated } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { session } from '#lib/session.svelte.ts';
	import { listen } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { cleanupLegacyServiceWorker, forgetLegacyScreenToken } from '#lib/legacy.ts';
	import { refocus } from '#lib/focus.ts';
	import Header from '#lib/components/Header.svelte';
	import SignIn from '#lib/components/SignIn.svelte';
	import AuthShell from '#lib/components/AuthShell.svelte';
	import Door from '#lib/components/Door.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Toasts from '#lib/components/Toasts.svelte';
	import ScreenPairing from '#lib/components/ScreenPairing.svelte';
	import SunTheme from '#lib/components/SunTheme.svelte';
	import { keptSide, lastSide } from '#lib/sun/data.ts';

	let { children } = $props();
	// a removed screen, paired again (ScreenPairing says its own title)
	let repairing = $state(false);

	// an invitation opens signed out: it is how one gets a first passkey
	const invitation = $derived(page.url.pathname === '/invite' || page.url.pathname.startsWith('/invite/'));

	$effect(() => {
		void session.verify();
		void cleanupLegacyServiceWorker();
		forgetLegacyScreenToken();
	});

	// the sun's theme: chosen, or a wall screen's own when nothing is (nobody there to choose)
	const bySun = $derived(ui.theme === 'sun' || (ui.theme === 'system' && session.screen !== null));
	// where its side comes from: the house's place for someone in, the side alone signed out (a
	// screen waiting to be paired, its theme chosen by its address)
	const sunFrom = $derived(!bySun ? null : session.status === 'signed_in' ? 'house' : session.status === 'loading' ? null : 'outside');
	$effect(() => {
		const root = document.documentElement;
		// following the sun: what it says, else what it last said while that holds (no daylight
		// flash while it is asked); not following it any more (the session known: a screen is one
		// only once it says so), the kept side goes
		if (!bySun && session.status !== 'loading') lastSide.set(undefined);
		const side = bySun ? (ui.sun === undefined ? keptSide() : ui.sun) : ui.theme === 'system' ? null : ui.theme;
		if (side) root.dataset.theme = side;
		else delete root.dataset.theme;
	});

	// someone, or a screen, in: the server says what changed (pushed values read at once)
	$effect(() => {
		if (session.status === 'signed_in') return listen();
	});

	// signed in (a passkey, an invitation): the page replaces the door under the focus, which
	// goes to the page's title
	let was = session.status;
	$effect(() => {
		const now = session.status;
		if (now === 'signed_in' && was !== 'signed_in' && was !== 'loading') void refocus('main h1');
		was = now;
	});

	// on a change of view, focus goes to the new view's title
	let first = true;
	afterNavigate(() => {
		if (first) return void (first = false);
		void refocus('main h1');
	});

	// a new version deployed: taken at the next harmless moment, never under the fingers
	beforeNavigate(({ willUnload, to }) => {
		if (updated.current && !willUnload && to?.url) location.href = to.url.href;
	});
	$effect(() => {
		const typing = () => !!document.activeElement?.closest('input, textarea, select, [contenteditable]');
		const back = async () => {
			if (document.visibilityState !== 'visible') return;
			if ((updated.current || (await updated.check())) && !typing()) location.reload();
		};
		document.addEventListener('visibilitychange', back);
		return () => document.removeEventListener('visibilitychange', back);
	});
</script>

<nav class="skip" aria-label={m.skip_to_content()}><a href="#main">{m.skip_to_content()}</a></nav>
{#if session.status === 'signed_in'}<Header />{/if}
{#if sunFrom}{#key sunFrom}<SunTheme house={sunFrom === 'house'} />{/key}{/if}
<main id="main" class:page={session.status === 'signed_in'} tabindex="-1">
	{#if session.status === 'loading'}
		<p class="muted loading">{m.common_loading()}</p>
	{:else if session.status === 'unreachable'}
		<AuthShell>
			<Door title={m.unreachable_title()}>
				<p class="lead">{m.unreachable_body()}</p>
				<button class="btn primary big" onclick={() => location.reload()}><Icon name="refresh-cw" />{m.reload()}</button>
			</Door>
		</AuthShell>
	{:else if session.status === 'revoked'}
		<AuthShell>
			{#if repairing}
				<ScreenPairing />
			{:else}
				<Door title={m.screen_revoked_title()}>
					<p class="lead">{m.screen_revoked_body()}</p>
					<div><button class="btn primary big" onclick={() => (repairing = true)}><Icon name="tv" />{m.screen_pair_retry()}</button></div>
				</Door>
			{/if}
		</AuthShell>
	{:else if session.status === 'signed_out' && !invitation}
		<SignIn />
	{:else}
		{@render children()}
	{/if}
</main>
<Toasts />
<!-- two live regions mounted once; only their text changes: the one status region (§ 4), and
     the alert for a failure -->
<div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{ui.polite}</div>
<div class="sr-only" role="alert">{ui.assertive}</div>

<style>
	main {
		padding-bottom: calc(var(--bottom-h) + var(--s-6));
	}
	.loading {
		padding: var(--s-6) var(--page-margin);
	}
	/* the door fills the screen: no room kept for a bottom bar it does not have */
	main:not(.page) {
		padding-bottom: 0;
	}
</style>
