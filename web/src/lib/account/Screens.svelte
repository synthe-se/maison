<script lang="ts">
	// The house's wall screens (an admin's, docs/passkeys.md « Wall screens »): each paired one, when
	// it was and when it was last seen; pairing one from the code it shows (from its own network,
	// the house's Wi-Fi), in a sheet; removing one asks first (§ 6), then the focus goes to the
	// list's title. A code refused is said under its field, a name taken under the name.
	import { m } from '#lib/paraglide/messages.js';
	import { ApiError } from '#lib/api.ts';
	import { MAX_NAME, SCREEN_CODE_LENGTH } from '#lib/limits.ts';
	import { longDate, when } from '#lib/i18n.svelte.ts';
	import { live } from '#lib/live.svelte.ts';
	import { ui } from '#lib/ui.svelte.ts';
	import { Gesture, pending } from '#lib/gesture.svelte.ts';
	import { refocus } from '#lib/focus.ts';
	import { Draft } from '#lib/draft.svelte.ts';
	import { errorText } from '#lib/errors.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Group from '#lib/components/Group.svelte';
	import ListRow from '#lib/components/ListRow.svelte';
	import Loaded from '#lib/components/Loaded.svelte';
	import Sheet from '#lib/components/Sheet.svelte';
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
		codeError = code ? '' : m.screens_code_missing();
		nameError = name ? '' : m.screens_name_missing();
		if (codeError) return codeField?.focus();
		if (nameError) return nameField?.focus();
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
		g.run(
			() => screensApi.remove(s.id),
			async () => {
				ui.toast(m.screens_removed({ name: s.name }));
				await screens.refresh();
				await refocus(title);
			},
			`remove:${s.id}`
		);
</script>

<Group id="screens-title" title={m.screens_title()} bind:heading={title}>
	<p class="hint">{m.screens_intro()}</p>
	<div><button class="btn primary" aria-haspopup="dialog" onclick={() => (pairing = true)}>{m.screens_pair()}</button></div>
	<Sheet open={pairing} title={m.screens_pair()} onclose={close} {draft}>
		<form class="pair" onsubmit={(e) => (e.preventDefault(), void pair())} novalidate>
			<div class="field">
				<label for="screen-code">{m.screens_code()}</label>
				<input
					id="screen-code"
					bind:this={codeField}
					bind:value={form.code}
					oninput={() => (codeError = '')}
					maxlength={SCREEN_CODE_LENGTH}
					autocomplete="off"
					autocapitalize="characters"
					spellcheck="false"
					class="code"
					aria-invalid={codeError ? 'true' : undefined}
					aria-describedby="screen-code-error"
				/>
				<p class="form-error" id="screen-code-error">{codeError}</p>
			</div>
			<div class="field">
				<label for="screen-name">{m.screens_name()}</label>
				<input
					id="screen-name"
					bind:this={nameField}
					bind:value={form.name}
					oninput={() => (nameError = '')}
					maxlength={MAX_NAME}
					autocomplete="off"
					aria-invalid={nameError ? 'true' : undefined}
					aria-describedby="screen-name-error"
				/>
				<p class="form-error" id="screen-name-error">{nameError}</p>
			</div>
			<div class="actions">
				<button class="btn primary" type="submit" {...pending(g.is('pair'))}>{m.screens_pair()}</button>
			</div>
		</form>
	</Sheet>
	<Loaded value={screens}>
		{#if screens.data?.length}
			<ul class="plain-list">
				{#each screens.data as s (s.id)}
					<ListRow
						second="{m.screens_since({ when: longDate(s.createdMs) })} · {s.lastUsedMs
							? m.screens_used({ when: when(s.lastUsedMs) })
							: m.screens_never_used()}"
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
		{:else}
			<p class="hint">{m.screens_none()}</p>
		{/if}
	</Loaded>
</Group>

<style>
	.pair {
		display: grid;
		gap: var(--s-4);
	}
	.code {
		text-transform: uppercase;
		letter-spacing: 0.1em;
	}
</style>
