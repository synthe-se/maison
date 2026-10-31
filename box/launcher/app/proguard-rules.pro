# The feed's @Serializable types, read by their generated serializers.
-keepclassmembers class maison.box.**$$serializer { *; }
-keepclasseswithmembers class maison.box.** {
	kotlinx.serialization.KSerializer serializer(...);
}
