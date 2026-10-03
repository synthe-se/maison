// The choices of a Select or a ToggleGroup, from the values and their words, in the reader's
// language: call it where it renders (in the template), never once at load, so a change of
// language reaches it.

export interface Option<T extends string> {
	value: T;
	label: string;
}

/** `values` in order, each with its label (a map of messages, or a function of the value). */
export function options<T extends string>(
	values: readonly T[],
	label: Readonly<Record<T, () => string>> | ((v: T) => string)
): Option<T>[] {
	return values.map((value) => ({ value, label: typeof label === 'function' ? label(value) : label[value]() }));
}
