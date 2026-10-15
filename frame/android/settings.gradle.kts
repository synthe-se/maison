pluginManagement {
	repositories {
		google()
		mavenCentral()
		gradlePluginPortal()
	}
}

dependencyResolutionManagement {
	repositoriesMode = RepositoriesMode.FAIL_ON_PROJECT_REPOS
	repositories {
		google()
		mavenCentral()
		maven("https://maven.mozilla.org/maven2/") {
			content { includeGroup("org.mozilla.geckoview") }
		}
	}
}

rootProject.name = "maison-frame"
include(":app")
