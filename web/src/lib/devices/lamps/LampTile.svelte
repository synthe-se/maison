<script lang="ts">
	// A lamp's tile, Hue or Zigbee alike (docs/ux.md § 1–3): one compact row, the icon turns
	// it on or off, the name opens its page, line 2 says « Allumée, 80 % ». The brightness
	// slider only under a lamp that is on and reachable: an off or unreachable lamp shows no
	// slider that looks draggable. Unreachable: said in words with since when, the gesture kept
	// in place but unavailable (§ 4).
	import { m } from '#lib/paraglide/messages.js';
	import { percent } from '#lib/i18n.svelte.ts';
	import { Command, LIMIT } from '#lib/command.svelte.ts';
	import { refresh } from '#lib/live.svelte.ts';
	import DeviceTile from '#lib/components/DeviceTile.svelte';
	import Range from '#lib/components/Range.svelte';
	import { lampState, type Lamp, type LampDriver } from './lamp.ts';

	interface Props {
		lamp: Lamp;
		driver: LampDriver;
		/** On the lamp's own page the name is the title, not a link. */
		link?: boolean;
	}
	let { lamp, driver, link = true }: Props = $props();

	const command = new Command(() => lamp.name, LIMIT.lamp);
	const settle = () => refresh(driver.key);

	function toggle(next: boolean) {
		void command.run(next, async () => {
			await driver.power(lamp.id, next);
			await settle();
		});
	}
</script>

<DeviceTile
	level={link ? 3 : 2}
	name={lamp.name}
	icon={lamp.reachable ? 'lightbulb' : 'lightbulb-off'}
	state={lampState(lamp)}
	href={link ? driver.href(lamp.id) : undefined}
	warn={!lamp.reachable && !lamp.connecting}
	on={lamp.isOn}
	offline={!lamp.reachable}
	{command}
	ontoggle={toggle}
>
	{#if lamp.reachable && lamp.isOn}
		<Range
			live
			near={2}
			label={m.lamps_brightness()}
			hideLabel
			value={lamp.brightness}
			min={1}
			valueText={(v) => percent(v / 100)}
			send={(v) => driver.brightness(lamp.id, v)}
			{settle}
		/>
	{/if}
</DeviceTile>
