// Snippets for component props in tests (`children`, `end`, `panel`…).

import { createRawSnippet } from 'svelte';

/** A snippet that renders a fixed piece of markup (one root element). */
export const html = (markup: string) => createRawSnippet(() => ({ render: () => markup }));

/** A snippet that renders `<span>text</span>`. */
export const text = (s: string) => html(`<span>${s}</span>`);

/** A snippet with one argument, rendered by `render(arg)` (one root element). */
export const htmlOf = <T>(render: (arg: T) => string) =>
	createRawSnippet((arg: () => T) => ({ render: () => render(arg()) }));
