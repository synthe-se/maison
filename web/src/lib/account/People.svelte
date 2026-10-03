<script lang="ts">
	// Who may come in (an admin's): each person, an admin marked, their passkeys counted;
	// removing one asks first (§ 6: their passkeys and sessions go), never oneself. The focus
	// then goes to the list's title.
	import { m } from '#lib/paraglide/messages.js';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { session } from '#lib/session.svelte.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import { peopleApi, peopleData, type Person } from '#lib/passkeys.ts';

	const people = live(peopleData);
	const g = new Gesture();
	let title = $state<HTMLElement>();

	const remove = (p: Person) =>
		g.run(
			() => peopleApi.remove(p.id),
			async () => {
				ui.toast(m.people_removed({ name: p.name }));
				await people.refresh();
				await refocus(title);
			},
			p.id
		);
</script>

<Group id="people-title" title={m.people_title()} bind:heading={title}>
	<p class="hint">{m.people_intro()}</p>
	<Loaded value={people}>
		<ul class="plain-list">
			{#each people.data ?? [] as p (p.id)}
				<ListRow second={m.people_passkeys({ count: p.passkeys })}>
					<span>{p.name}</span>{#if p.role === 'admin'}<span class="chip">{m.people_admin()}</span>{/if}
					{#snippet end()}
						{#if p.id === session.user?.id}
							<span class="hint">{m.people_you()}</span>
						{:else}
							<ConfirmDialog
								ghost
								danger
								label={m.common_remove()}
								ariaLabel={m.common_remove_named({ name: p.name })}
								title={m.common_remove_from_maison({ name: p.name })}
								description={m.people_remove_description()}
								action={m.people_remove_action()}
								busy={g.is(p.id)}
								onconfirm={() => remove(p)}
							/>
						{/if}
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	</Loaded>
</Group>
