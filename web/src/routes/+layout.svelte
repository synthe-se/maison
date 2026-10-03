<script lang="ts">
	import '@fontsource-variable/fraunces';
	import '../styles/app.css';
	import '#lib/i18n.svelte.ts';
	import { tick } from 'svelte';
	import { afterNavigate, beforeNavigate } from '$app/navigation';
	import { page, updated } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { session } from '#lib/session.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { cleanupLegacyServiceWorker } from '#lib/legacy.ts';
	import { refocus } from '#lib/focus.ts';
	import Header from '#lib/components/Header.svelte';
	import SignIn from '#lib/components/SignIn.svelte';
	import Toasts from '#lib/components/Toasts.svelte';

	let { children } = $props();

	// an invitation opens signed out: it is how one gets a first passkey
	const invitation = $derived(page.url.pathname.startsWith('/invite/'));

	$effect(() => {
		void session.verify();
		void cleanupLegacyServiceWorker();
	});

	$effect(() => {
		const root = document.documentElement;
		if (ui.theme === 'system') delete root.dataset.theme;
		else root.dataset.theme = ui.theme;
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
	afterNavigate(async () => {
		if (first) return void (first = false);
		await tick();
		document.querySelector<HTMLElement>('main h1')?.focus();
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

<!-- the screens before a page keep a title of their own, not the asked page's -->
<svelte:head>
	{#if session.status === 'unreachable'}<title>{m.unreachable_title()} · {m.branding_name()}</title>{/if}
</svelte:head>

<nav class="skip" aria-label={m.skip_to_content()}><a href="#main">{m.skip_to_content()}</a></nav>
{#if session.status === 'signed_in'}<Header />{/if}
<main id="main" class:page={session.status === 'signed_in'} tabindex="-1">
	{#if session.status === 'loading'}
		<p class="muted loading">{m.common_loading()}</p>
	{:else if session.status === 'unreachable'}
		<section class="notice">
			<h1 tabindex="-1">{m.unreachable_title()}</h1>
			<p>{m.unreachable_body()}</p>
			<button class="btn primary" onclick={() => location.reload()}>{m.reload()}</button>
		</section>
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
	main { padding-bottom: calc(var(--bottom-h) + var(--s-6)); }
	.loading { padding: var(--s-6) var(--page-margin); }
	.notice { max-width: var(--measure); margin: 12vh auto 0; padding-inline: var(--page-margin); display: grid; gap: var(--s-3); justify-items: start; }
	.notice h1 { font: var(--t-page); }
	.notice p { margin: 0; }
</style>
