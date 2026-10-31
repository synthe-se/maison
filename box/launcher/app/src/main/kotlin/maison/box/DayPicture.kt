package maison.box

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** How wide a picture must be for its height: a portrait would be cut to a band of it. */
private const val MIN_RATIO = 1.3
/** Longer than this, an « artist » field is a note on the file, not a name. */
private const val MAX_NAME = 60
private const val FEED_THUMB = "960px-"
private const val WALL_THUMB = "1920px-"

private val feedJson = Json { ignoreUnknownKeys = true }

// What is read of Wikimedia's featured feed.
@Serializable private data class Feed(val image: FeedImage? = null)
@Serializable private data class FeedImage(
	val title: String = "",
	val image: Size = Size(),
	val thumbnail: Thumbnail = Thumbnail(),
	val artist: Words = Words(),
	val license: License = License(),
)
@Serializable private data class Size(val width: Int = 0, val height: Int = 0)
@Serializable private data class Thumbnail(val source: String = "")
@Serializable private data class Words(val text: String = "")
@Serializable private data class License(val type: String = "")

/** The day's picture, and what Commons asks to say of it: its author and its license. */
internal data class DayPicture(val url: String, val caption: String, val author: String, val license: String) {
	val credit get() = listOf(caption, author, license).joinToString(" · ")
}

/**
 * The picture of a day of the feed, when it can dress a screen: landscape, and credited in a name
 * and a license. No clean credit, no picture: another day's will do.
 */
internal fun dayPicture(feed: String): DayPicture? {
	val image = feedJson.decodeFromString<Feed>(feed).image ?: return null
	val author = image.artist.text.trim()
	val fits = image.image.width >= image.image.height * MIN_RATIO && image.image.width > 0
	val credited = author.isNotEmpty() && author.length <= MAX_NAME && image.license.type.isNotEmpty()
	if (!fits || !credited || FEED_THUMB !in image.thumbnail.source) return null
	return DayPicture(image.thumbnail.source.replace(FEED_THUMB, WALL_THUMB), caption(image.title), author, image.license.type)
}

/** `File:Vignes près du Pic - 2020.jpg` as a line: its name, without the file. */
internal fun caption(title: String) = title.removePrefix("File:").substringBeforeLast('.').replace('_', ' ')
