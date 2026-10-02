<script lang="ts">
	// From 600 px a sticky 56 px header whose inner row is the page container; below, the
	// destinations move to a bottom bar (docs/ux/mise-en-page.md § 8). Session and display
	// settings live in one panel, from the person's name.
	import { Popover } from 'bits-ui';
	import { page } from '$app/state';
	import { m } from '#lib/paraglide/messages.js';
	import { session } from '#lib/session.svelte.ts';
	import { ui, type Theme } from '#lib/ui.svelte.ts';
	import { locale, localeName, locales, switchLocale } from '#lib/i18n.svelte.ts';
	import Brand from './Brand.svelte';
	import Icon, { type IconName } from './Icon.svelte';

	const views: { href: string; label: () => string; icon: IconName; match: (p: string) => boolean }[] = [
		{ href: '/', label: m.nav_home, icon: 'house', match: (p) => p === '/' || /^\/(device|hue-lamp|zigbee-lamp|meross)\//.test(p) },
		{ href: '/remote', label: m.nav_remote, icon: 'radio', match: (p) => p.startsWith('/remote') },
		{ href: '/tempo-predictions', label: m.nav_tempo, icon: 'zap', match: (p) => p.startsWith('/tempo') }
	];
	const themes: { value: Theme; label: () => string }[] = [
		{ value: 'system', label: m.theme_system },
		{ value: 'light', label: m.theme_light },
		{ value: 'dark', label: m.theme_dark }
	];
	const name = $derived(session.user?.username ?? '');
</script>

{#snippet nav(cls: string)}
	<nav class={cls} aria-label={m.nav_label()}>
		{#each views as v (v.href)}
			<a href={v.href} aria-current={v.match(page.url.pathname) ? 'page' : undefined}>
				<Icon name={v.icon} size={20} />
				<span>{v.label()}</span>
			</a>
		{/each}
	</nav>
{/snippet}

<header class="top">
	<div class="page bar">
		<Brand />
		{@render nav('views')}
		<div class="grow"></div>
		<Popover.Root>
			<Popover.Trigger class="btn ghost who" aria-label={m.session_menu({ name })}>
				<span class="initial" aria-hidden="true">{name.slice(0, 1).toUpperCase()}</span>
				<span class="who-name">{name}</span>
			</Popover.Trigger>
			<Popover.Portal>
				<Popover.Content class="panel" sideOffset={6} align="end" role="dialog" aria-labelledby="session-title">
					<h2 id="session-title" class="who-title">{name}</h2>
					<div role="group" aria-labelledby="lang-label">
						<h3 class="label" id="lang-label">{m.language_label()}</h3>
						<div class="row">
							{#each locales as l (l)}
								<button class="btn" lang={l} aria-pressed={locale() === l} onclick={() => switchLocale(l)}>{localeName(l)}</button>
							{/each}
						</div>
					</div>
					<div role="group" aria-labelledby="theme-label">
						<h3 class="label" id="theme-label">{m.theme_label()}</h3>
						<div class="row">
							{#each themes as t (t.value)}
								<button class="btn" aria-pressed={ui.theme === t.value} onclick={() => ui.setTheme(t.value)}>{t.label()}</button>
							{/each}
						</div>
					</div>
					<div class="row">
						<button class="btn" onclick={() => session.signOut()}>
							<Icon name="log-out" />{m.auth_logout()}
						</button>
					</div>
				</Popover.Content>
			</Popover.Portal>
		</Popover.Root>
	</div>
</header>
{@render nav('bottom')}

<style>
	.top { position: sticky; top: 0; z-index: 20; border-bottom: 1px solid var(--line); background: var(--ground); }
	.bar { display: flex; align-items: center; gap: var(--s-3) var(--s-5); min-height: var(--header-h); }
	.grow { flex: 1; }
	.views { display: flex; gap: var(--s-1); }
	.views a, .bottom a { display: inline-flex; align-items: center; gap: var(--s-2); color: var(--ink-muted); text-decoration: none; font: var(--t-label); border-radius: var(--radius); }
	.views a { min-height: var(--control-h-s); padding: 0 var(--s-3); }
	.views a:hover { background: var(--ground-raised); color: var(--ink); }
	.views a[aria-current='page'] { color: var(--accent); background: var(--accent-wash); }
	:global(.who) { gap: var(--s-2); }
	.initial { display: inline-grid; place-items: center; width: 26px; height: 26px; border-radius: 50%; background: var(--accent-wash); color: var(--accent); font: var(--t-meta); }
	:global(.panel .who-title) { font: var(--t-group); margin: 0; }
	:global(.panel .label) { font-family: var(--font-text); font-size: 0.85rem; font-weight: 600; margin-bottom: 6px; }

	/* the bottom bar: navigation only, always visible, under 600 px */
	.bottom { display: none; }
	@media (max-width: 599px) {
		.views, .who-name { display: none; }
		.bottom {
			position: fixed; inset: auto 0 0 0; z-index: 20; display: grid; grid-auto-flow: column; grid-auto-columns: 1fr;
			height: var(--bottom-h); padding-bottom: env(safe-area-inset-bottom, 0px); border-top: 1px solid var(--line); background: var(--ground);
		}
		.bottom a { flex-direction: column; justify-content: center; gap: var(--s-1); font: var(--t-meta); }
		.bottom a[aria-current='page'] { color: var(--accent); }
	}
</style>
