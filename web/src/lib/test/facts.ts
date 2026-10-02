// What a description list says (`<dl class="facts">` of StatusRow): each term, its value in
// words, and whether it carries the warning sign.

export type Fact = { term: string; value: string; warn: boolean };

export function facts(root: ParentNode = document): Fact[] {
	return [...root.querySelectorAll('dt')].map((dt) => {
		const dd = dt.nextElementSibling as HTMLElement | null;
		return { term: dt.textContent?.trim() ?? '', value: dd?.textContent?.trim() ?? '', warn: !!dd?.querySelector('svg') };
	});
}

/** The one fact under `term` (the first, when several share it). */
export const fact = (term: string, root: ParentNode = document) => facts(root).find((f) => f.term === term);
