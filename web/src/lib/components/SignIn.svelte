<script lang="ts">
	// A centred 25rem card, no header to align with (docs/ux/mise-en-page.md § 3).
	import { m } from '#lib/paraglide/messages.js';
	import { session } from '#lib/session.svelte.ts';
	import Brand from './Brand.svelte';

	let username = $state('');
	let password = $state('');
	let busy = $state(false);
	let error = $state('');

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		busy = true;
		error = '';
		try {
			await session.signIn(username, password);
		} catch (err) {
			error = err instanceof Error && err.message ? err.message : m.auth_invalid_credentials();
		} finally {
			busy = false;
		}
	}
</script>

<div class="signin">
	<Brand />
	<h1 tabindex="-1">{m.auth_login()}</h1>
	<p class="hint">{m.auth_login_card_description()}</p>
	<form onsubmit={submit} class="form">
		<div class="field">
			<label for="username">{m.auth_username()}</label>
			<input id="username" bind:value={username} autocomplete="username" autocapitalize="none" required />
		</div>
		<div class="field">
			<label for="password">{m.auth_password()}</label>
			<input id="password" type="password" bind:value={password} autocomplete="current-password" required />
		</div>
		<p class="form-error" role="alert">{error}</p>
		<button class="btn primary submit" disabled={busy}>{busy ? m.auth_logging_in() : m.auth_login()}</button>
	</form>
</div>

<style>
	.signin { width: min(25rem, calc(100% - 2 * var(--page-margin))); margin: 12vh auto 0; display: grid; gap: var(--s-3); }
	h1 { font: var(--t-page); margin-top: var(--s-5); }
	.form { display: grid; gap: var(--s-4); margin-top: var(--s-3); }
	.submit { min-height: var(--control-h); justify-content: center; }
</style>
