package maison.box

/** How many apps a home starts with, before anyone chose. */
private const val FIRST_FAVORITES = 5
private const val SEPARATOR = ","

/** The favorites saved, those still installed; before any choice, the first apps the person installed. */
internal fun favoritesOf(saved: String?, apps: List<BoxApp>): List<String> {
	val installed = apps.map { it.packageName }
	return saved?.split(SEPARATOR)?.filter { it in installed }
		?: apps.filterNot { it.system }.take(FIRST_FAVORITES).map { it.packageName }
}

internal fun List<String>.saved() = joinToString(SEPARATOR)

internal fun List<String>.with(name: String) = if (name in this) this else this + name

/** [name] [by] places along, stopped at the ends. */
internal fun List<String>.moved(name: String, by: Int): List<String> {
	val from = indexOf(name)
	if (from < 0) return this
	val to = (from + by).coerceIn(indices)
	return toMutableList().apply { add(to, removeAt(from)) }
}
