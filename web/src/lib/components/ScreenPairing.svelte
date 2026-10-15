<script lang="ts">
	// This browser becoming a wall screen (docs/passkeys.md « Wall screens »): the code to give
	// from a phone, big enough to read across the room, asked about every few seconds; paired, the
	// screen opens by itself. The pairing's secret stays in this component's memory, never in an
	// address nor in storage. A code past its time offers a new one; leaving stops the asking.
	import { m } from '#lib/paraglide/messages.js';
	import { ApiError } from '#lib/api.ts';
	import { errorText } from '#lib/errors.ts';
	import { refocus } from '#lib/focus.ts';
	import { pageTitle } from '#lib/i18n.svelte.ts';
	import { screensApi, type Pairing } from '#lib/screens.ts';
	import { session } from '#lib/session.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import Icon from './Icon.svelte';

	interface Props {
		/** Back to what was there before (the sign-in). */
		onback?: () => void;
	}
	let { onback }: Props = $props();

	let pairing = $state.raw<Pairing | null>(null);
	let expired = $state(false);
	let failure = $state('');
	// one pairing at a time: a new one (or leaving) ends the asking of the one before
	let round = 0;

	async function start() {
		const mine = ++round;
		pairing = null;
		expired = false;
		failure = '';
		let p: Pairing;
		try {
			p = await screensApi.start();
		} catch (e) {
			if (mine === round) failure = errorText(e);
			return;
		}
		if (mine !== round) return;
		pairing = p;
		const until = Date.now() + p.expiresInMs;
		while (mine === round) {
			await new Promise((resolve) => setTimeout(resolve, p.pollMs));
			if (mine !== round) return;
			if (Date.now() >= until) return end();
			try {
				const r = await screensApi.claim(p.secret);
				if (mine !== round) return;
				if (r.paired && r.screen) {
					round++;
					ui.say(m.screen_paired({ name: r.screen.name }));
					return session.becomeScreen(r.screen.name);
				}
			} catch (e) {
				if (mine !== round) return;
				// its time is over on the server's side: a new code is one press away
				if (e instanceof ApiError && e.code === 'pairing_unknown') return end();
				// anything else (the Wi-Fi a moment away): asked again at the next round
			}
		}
	}

	async function end() {
		round++;
		expired = true;
		await refocus('#pair-again');
	}

	$effect(() => {
		void start();
		return () => void round++;
	});
</script>

<svelte:head><title>{pageTitle(m.screen_pair_title())}</title></svelte:head>

<section class="pairing" aria-labelledby="pair-title">
	<h1 id="pair-title" tabindex="-1">{m.screen_pair_title()}</h1>
	{#if failure}
		<p class="form-error" role="alert">{failure}</p>
		<div><button class="btn primary big" onclick={() => void start()}><Icon name="refresh-cw" />{m.screen_pair_again()}</button></div>
	{:else if expired}
		<p class="lead" role="status">{m.screen_pair_expired()}</p>
		<div>
			<button id="pair-again" class="btn primary big" onclick={() => void start()}><Icon name="refresh-cw" />{m.screen_pair_again()}</button
			>
		</div>
	{:else if pairing}
		<p class="lead">{m.screen_pair_intro()}</p>
		<!-- shown big; read out one character at a time -->
		<p class="code" aria-hidden="true">{pairing.code}</p>
		<p class="sr-only">{m.screen_pair_code()} {pairing.code.split('').join(' ')}</p>
		<p class="hint">{m.screen_pair_valid({ minutes: Math.round(pairing.expiresInMs / 60_000) })}</p>
	{:else}
		<p class="muted">{m.common_loading()}</p>
	{/if}
	{#if onback}
		<div><button class="btn ghost" onclick={onback}><Icon name="arrow-left" />{m.screen_pair_back()}</button></div>
	{/if}
</section>

<style>
	.pairing {
		display: grid;
		gap: var(--s-4);
	}
	.pairing p {
		margin: 0;
	}
	/* read across the room: large, its characters apart */
	.code {
		font: var(--t-code);
		letter-spacing: 0.15em;
		font-variant-numeric: tabular-nums;
		color: var(--accent);
	}
</style>
