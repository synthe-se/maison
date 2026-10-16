<script lang="ts">
	// My sessions (each sign-in kept by the server, the latest first): the device and browser in
	// words, when it was opened, when last seen and from where; « Cet appareil » marks this one.
	// Ending another one is at once (it can sign in again with its passkey); ending this one asks
	// first (§ 6), then the page is signed out (the server cleared its cookies). The focus then
	// goes to the list's title.
	import { m } from '#lib/paraglide/messages.js';
	import { browserName, lifeLine } from './lines.ts';
	import { facts } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { Gesture, pending, removeRow } from '#lib/gesture.svelte.ts';
	import { session } from '#lib/session.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { sessionsApi, sessionsData, type SessionInfo } from '#lib/passkeys.ts';

	const sessions = live(sessionsData);
	const g = new Gesture();
	let title = $state<HTMLElement>();

	/** « iPhone · Safari », or what is known of it. */
	const deviceName = (s: SessionInfo) => facts(s.device, browserName(s.userAgent)) || m.sessions_unknown_device();

	const endOther = (s: SessionInfo) =>
		removeRow(g, s.id, () => sessionsApi.end(s.id), { said: m.sessions_ended(), list: sessions, title: () => title });

	// this browser's own: its cookies are gone with it
	const endCurrent = (s: SessionInfo) =>
		g.run(
			() => sessionsApi.end(s.id),
			() => session.ended(),
			s.id
		);
</script>

<Group id="sessions-title" title={m.sessions_title()} bind:heading={title}>
	<p class="hint">{m.sessions_intro()}</p>
	<Loaded value={sessions}>
		<ul class="plain-list">
			{#each sessions.data ?? [] as s (s.id)}
				<ListRow
					second={facts(lifeLine(s.createdMs, s.lastSeenMs, { since: m.sessions_since, used: m.sessions_seen, never: () => '' }), s.ip)}
				>
					<span>{deviceName(s)}</span>{#if s.current}<span class="chip accent">{m.sessions_this_device()}</span>{/if}
					{#snippet end()}
						{#if s.current}
							<ConfirmDialog
								ghost
								label={m.sessions_end()}
								ariaLabel={m.sessions_end_label({ device: deviceName(s) })}
								title={m.sessions_end_current_title()}
								description={m.sessions_end_current_description()}
								action={m.sessions_end_current_action()}
								busy={g.is(s.id)}
								onconfirm={() => endCurrent(s)}
							/>
						{:else}
							<button
								class="btn ghost"
								aria-label={m.sessions_end_label({ device: deviceName(s) })}
								{...pending(g.is(s.id))}
								onclick={() => endOther(s)}>{m.sessions_end()}</button
							>
						{/if}
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	</Loaded>
</Group>
