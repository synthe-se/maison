<script lang="ts">
	// The house's wall screens (an admin's, docs/passkeys.md « Wall screens »): each paired one, when
	// it was and when it was last seen; pairing one from the code it shows (from its own network,
	// the house's Wi-Fi), in a sheet; removing one asks first (§ 6), then the focus goes to the
	// list's title. A code refused is said under its field, a name taken under the name.
	import { m } from '#lib/paraglide/messages.js';
	import { ApiError } from '#lib/api.ts';
	import { MAX_NAME, SCREEN_CODE_LENGTH } from '#lib/limits.ts';
	import { lifeLine } from './lines.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending, removeRow } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import { errorText } from '#lib/errors.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
	import TextField from '#lib/components/TextField.svelte';
	import { missingFields } from '#lib/form.ts';
	import { screensApi, screensData, type Screen } from '#lib/screens.ts';

	const screens = live(screensData);
	const g = new Gesture();
	let title = $state<HTMLElement>();

	let pairing = $state(false);
	const EMPTY = { code: '', name: '' };
	const draft = new Draft({ ...EMPTY });
	const form = $derived(draft.current);
	// what is wrong, under the field it is about
	let codeError = $state('');
	let nameError = $state('');
	let codeField = $state<HTMLInputElement>();
	let nameField = $state<HTMLInputElement>();

	function close() {
		pairing = false;
		codeError = nameError = '';
	}

	async function pair() {
		const code = form.code.trim();
		const name = form.name.trim();
		const { errors, first } = missingFields({ code, name }, { code: m.screens_code_missing, name: m.screens_name_missing });
		codeError = errors.code ?? '';
		nameError = errors.name ?? '';
		if (first) return (first === 'code' ? codeField : nameField)?.focus();
		const r = await g.run(() => screensApi.pair(code, name), undefined, 'pair', {
			field: () => codeField,
			// a refusal about the name is the name's field's; anything else, the code's
			refused: (e) => {
				if (e instanceof ApiError && e.code === 'screen_exists') {
					nameError = errorText(e);
					nameField?.select();
					return true;
				}
				codeError = errorText(e);
				codeField?.select();
				return true;
			}
		});
		if (!r) return;
		draft.reset({ ...EMPTY });
		close();
		ui.toast(m.screens_paired({ name: r.screen.name }));
		await screens.refresh();
		await refocus(title);
	}

	const remove = (s: Screen) =>
		removeRow(g, `remove:${s.id}`, () => screensApi.remove(s.id), {
			said: m.screens_removed({ name: s.name }),
			list: screens,
			title: () => title
		});
</script>

<Group id="screens-title" title={m.screens_title()} bind:heading={title}>
	<p class="hint">{m.screens_intro()}</p>
	<div><button class="btn primary" aria-haspopup="dialog" onclick={() => (pairing = true)}>{m.screens_pair()}</button></div>
	<Sheet open={pairing} title={m.screens_pair()} onclose={close} {draft}>
		<form class="pair" onsubmit={(e) => (e.preventDefault(), void pair())} novalidate>
			<TextField
				label={m.screens_code()}
				bind:input={codeField}
				bind:value={form.code}
				error={codeError}
				oninput={() => (codeError = '')}
				maxlength={SCREEN_CODE_LENGTH}
				autocomplete="off"
				autocapitalize="characters"
				spellcheck="false"
				class="code"
			/>
			<TextField
				label={m.screens_name()}
				bind:input={nameField}
				bind:value={form.name}
				error={nameError}
				oninput={() => (nameError = '')}
				maxlength={MAX_NAME}
				autocomplete="off"
			/>
			<div class="actions">
				<button class="btn primary" type="submit" {...pending(g.is('pair'))}>{m.screens_pair()}</button>
			</div>
		</form>
	</Sheet>
	<Loaded value={screens} empty={screens.data?.length === 0} emptyText={m.screens_none()}>
		<ul class="plain-list">
			{#each screens.data ?? [] as s (s.id)}
				<ListRow
					second={lifeLine(s.createdMs, s.lastUsedMs, { since: m.screens_since, used: m.screens_used, never: m.screens_never_used })}
				>
					<span>{s.name}</span>
					{#snippet end()}
						<ConfirmDialog
							ghost
							danger
							label={m.common_remove()}
							ariaLabel={m.common_remove_named({ name: s.name })}
							title={m.screens_remove_title({ name: s.name })}
							description={m.screens_remove_description()}
							action={m.screens_remove_action()}
							busy={g.is(`remove:${s.id}`)}
							onconfirm={() => remove(s)}
						/>
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	</Loaded>
</Group>

<style>
	.pair {
		display: grid;
		gap: var(--s-4);
	}
	.pair :global(.code) {
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}
</style>
