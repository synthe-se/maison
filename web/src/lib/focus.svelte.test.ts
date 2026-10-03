import { describe, expect, it } from 'vitest';
import { refocus, sectionHeading } from './focus.ts';

describe('refocus', () => {
	it('focuses the first target still on the page, making a heading focusable for it', async () => {
		document.body.innerHTML = '<section><h2>Volets</h2><button id="gone">x</button></section>';
		const gone = document.getElementById('gone')!;
		gone.remove();
		const heading = document.querySelector('h2')!;
		expect(await refocus(gone, heading)).toBe(heading);
		expect(document.activeElement).toBe(heading);
		expect(heading.getAttribute('tabindex')).toBe('-1');
	});

	it('takes a selector or a getter (an element the change itself brings)', async () => {
		document.body.innerHTML = '<input id="link" />';
		const brought: { el?: HTMLElement } = {};
		const done = refocus(() => brought.el);
		brought.el = document.getElementById('link')!;
		expect(await done).toBe(brought.el);
		document.body.innerHTML = '<input id="url" />';
		expect(await refocus('#url')).toBe(document.getElementById('url'));
	});

	it('leaves a control’s own tab order alone', async () => {
		document.body.innerHTML = '<button>Ajouter</button>';
		const button = document.querySelector('button')!;
		await refocus(button);
		expect(button.hasAttribute('tabindex')).toBe(false);
	});

	it('finds the title of the section around a row', () => {
		document.body.innerHTML = '<section><h2>Mes clés</h2><ul><li><button>Retirer</button></li></ul></section>';
		expect(sectionHeading(document.querySelector('button'))?.textContent).toBe('Mes clés');
	});
});
