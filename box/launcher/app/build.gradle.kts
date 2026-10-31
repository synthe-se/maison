import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
	alias(libs.plugins.android.application)
	alias(libs.plugins.kotlin.compose)
	alias(libs.plugins.kotlin.serialization)
	alias(libs.plugins.baselineprofile)
}

android {
	namespace = "maison.box"
	compileSdk = 37

	defaultConfig {
		applicationId = "maison.box"
		// Any Android TV from 10 on.
		minSdk = 29
		targetSdk = 37
		versionCode = 1
		versionName = "0.1.0"
	}

	// The release key never enters the repo: MAISON_BOX_SIGNING names a properties file
	// (storeFile, storePassword, keyAlias, keyPassword), ~/maison-box/keys/release.properties.
	val signing = System.getenv("MAISON_BOX_SIGNING")?.let { file(it) }?.takeIf { it.isFile }
	signingConfigs {
		if (signing != null) {
			create("release") {
				val p = Properties().apply { signing.inputStream().use(::load) }
				storeFile = file(p.getProperty("storeFile"))
				storePassword = p.getProperty("storePassword")
				keyAlias = p.getProperty("keyAlias")
				keyPassword = p.getProperty("keyPassword")
			}
		}
	}

	buildTypes {
		release {
			signingConfig = signingConfigs.findByName("release")
			isMinifyEnabled = true
			isShrinkResources = true
			proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
		}
	}

	compileOptions {
		sourceCompatibility = JavaVersion.VERSION_17
		targetCompatibility = JavaVersion.VERSION_17
	}

	lint {
		warningsAsErrors = true
		abortOnError = true
		// Pinned exactly, moved on purpose.
		disable += "GradleDependency"
		disable += "AndroidGradlePluginVersion"
		// A launcher's own job.
		disable += "QueryPermissionsNeeded"
	}

	buildFeatures { compose = true }

	// Robolectric renders the composables with the merged resources (fonts, drawables).
	testOptions { unitTests { isIncludeAndroidResources = true } }
}

// Generated on a real box (see baselineprofile/), committed under src/release/generated.
baselineProfile {
	automaticGenerationDuringBuild = false
}

kotlin {
	compilerOptions {
		jvmTarget = JvmTarget.JVM_17
		allWarningsAsErrors = true
	}
}

dependencies {
	implementation(platform(libs.compose.bom))
	implementation(libs.compose.foundation)
	implementation(libs.compose.ui)
	implementation(libs.tv.material)
	implementation(libs.activity.compose)
	implementation(libs.lifecycle.viewmodel.compose)
	implementation(libs.lifecycle.runtime.compose)
	implementation(libs.datastore.preferences)
	implementation(libs.work.runtime)
	implementation(libs.core)
	// Sideloaded: no Play cloud profile, this installs the committed one on first launch.
	implementation(libs.profileinstaller)
	baselineProfile(project(":baselineprofile"))
	implementation(libs.coroutines)
	implementation(libs.serialization.json)

	testImplementation(platform(libs.compose.bom))
	testImplementation(libs.junit)
	testImplementation(libs.kotlin.test)
	testImplementation(libs.robolectric)
	testImplementation(libs.compose.ui.test)
	debugImplementation(libs.compose.ui.test.manifest)
}
