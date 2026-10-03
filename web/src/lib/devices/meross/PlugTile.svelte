<script lang="ts">
	// One plug on the dashboard: its icon turns it on or off (state read back from the plug),
	// its name leads to its page, the live power on the right (docs/ux.md § 1).
	import { m } from '#lib/paraglide/messages.js';
	import { merossApi } from './api.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import { Command, LIMIT } from '#lib/command.svelte.ts';
	import { unreachableSince } from '#lib/format.ts';
	import { MEROSS } from './data.ts';
	import DeviceTile from '#lib/components/DeviceTile.svelte';

	interface Props {
		id: string;
		name: string;
		on: boolean;
		online: boolean;
		/** When it last answered (ms), to say since when it does not. */
		lastPing?: number;
		/** The live power, already in words (« 42 W »). */
		fact?: string;
		/** Said in place of « Allumée », in the warning style (drawing power in red peak hours). */
		nudge?: string;
		/** Off on the plug's own page, where the name is the page's title. */
		link?: boolean;
	}
	let { id, name, on, online, lastPing, fact, nudge, link = true }: Props = $props();

	const command = new Command(() => name, LIMIT.plug);

	function toggle(next: boolean) {
		void command.run(next, async () => {
			await merossApi.toggle(id, next);
			await refresh(MEROSS);
		});
	}
</script>

<DeviceTile
	level={link ? 3 : 2}
	{name}
	icon={on ? 'plug-zap' : 'plug'}
	href={link ? `/meross/${id}` : undefined}
	state={!online ? unreachableSince(lastPing) : on ? (nudge ?? m.state_on()) : m.state_off()}
	warn={!!nudge && online && on}
	offline={!online}
	{on}
	{command}
	ontoggle={toggle}
	{fact}
/>
