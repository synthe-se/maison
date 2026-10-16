// A sheet's form and its required fields, said once (an invitation, a wall screen, a shutter):
// each one left empty is said under it, and the focus goes to the first of them.

/** The words for each empty field among `values` (trimmed), and the first empty one in their
 * order; nothing missing: no words, no field. */
export function missingFields<K extends string>(
	values: Record<K, string>,
	words: Record<K, () => string>
): { errors: Partial<Record<K, string>>; first?: K } {
	const errors: Partial<Record<K, string>> = {};
	let first: K | undefined;
	for (const key of Object.keys(values) as K[]) {
		if (values[key].trim()) continue;
		errors[key] = words[key]();
		first ??= key;
	}
	return { errors, first };
}
