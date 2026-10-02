<script lang="ts">
	import { percent } from '#lib/i18n.svelte.ts';
	// A lamp's tile, Hue or Zigbee alike (docs/ux/tableau-de-bord.md § 1–3): the icon turns it
	// on or off, the name opens its page, line 2 says « Allumée, 80 % », and the brightness
	// slider follows the finger. Unreachable: said in words, the last value kept but greyed.
	import { m } from '#lib/paraglide/messages.js';
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
	on={lamp.reachable && lamp.isOn}
	command={lamp.reachable ? command : undefined}
	ontoggle={lamp.reachable ? toggle : undefined}
>
	<Range
		live
		near={2}
		label={lamp.reachable ? m.lamps_brightness() : m.lamps_last_brightness()}
		value={lamp.brightness}
		min={1}
		valueText={(v) => percent(v / 100)}
		send={(v) => driver.brightness(lamp.id, v)}
		{settle}
		disabled={!lamp.reachable || !lamp.isOn}
	/>
</DeviceTile>
