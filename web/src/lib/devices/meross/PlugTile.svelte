<script lang="ts">
	// One plug on the dashboard: its icon turns it on or off (state read back from the plug),
	// its name leads to its page, the live power on the right (docs/ux/tableau-de-bord.md § 1).
	import { m } from '#lib/paraglide/messages.js';
	import { merossApi } from '#lib/api.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import { Command, LIMIT } from '#lib/command.svelte.ts';
	import DeviceTile from '#lib/components/DeviceTile.svelte';

	interface Props {
		id: string;
		name: string;
		on: boolean;
		online: boolean;
		/** The live power, already in words (« 42 W »). */
		fact?: string;
		/** Off on the plug's own page, where the name is the page's title. */
		link?: boolean;
	}
	let { id, name, on, online, fact, link = true }: Props = $props();

	const command = new Command(() => name, LIMIT.plug);

	function toggle(next: boolean) {
		void command.run(next, async () => {
			await merossApi.toggle(id, next);
			await refresh('meross');
		});
	}
</script>

<DeviceTile
	level={link ? 3 : 2}
	{name}
	icon={on ? 'plug-zap' : 'plug'}
	href={link ? `/meross/${id}` : undefined}
	state={!online ? m.state_unreachable() : on ? m.meross_state_on() : m.state_off()}
	warn={!online}
	{on}
	command={online ? command : undefined}
	ontoggle={online ? toggle : undefined}
	{fact}
/>
