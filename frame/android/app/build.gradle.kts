import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
	alias(libs.plugins.android.application)
}

android {
	namespace = "maison.frame"
	compileSdk = 37

	defaultConfig {
		applicationId = "maison.frame"
		// The frame: Android 6.0.1 (RK3126, armeabi-v7a only).
		minSdk = 23
		targetSdk = 37
		versionCode = 1
		versionName = "0.1.0"
		ndk { abiFilters += "armeabi-v7a" }
	}

	// The release key never enters the repo: MAISON_FRAME_SIGNING names a properties file
	// (storeFile, storePassword, keyAlias, keyPassword), ~/maison-frame/keys/release.properties.
	val signing = System.getenv("MAISON_FRAME_SIGNING")?.let { file(it) }?.takeIf { it.isFile }
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

	buildFeatures { buildConfig = true }

	// Compressed in the APK and extracted, as Android 6 wants it of a system app: build.py lays
	// them out in /system/priv-app/MaisonFrame/lib/arm, where the package manager finds the ABI.
	packaging { jniLibs { useLegacyPackaging = true } }

	lint {
		warningsAsErrors = true
		abortOnError = true
		// GeckoView is held at 144 on purpose (see libs.versions.toml).
		disable += "GradleDependency"
		// One device, an ARM frame: no ChromeOS, no x86.
		disable += "ChromeOsAbiSupport"
		// ExoPlayer's notification helper, inside GeckoView: the frame posts no notification.
		disable += "NotificationPermission"
	}

	testOptions { unitTests.all { it.useJUnitPlatform() } }
}

kotlin {
	compilerOptions {
		jvmTarget = JvmTarget.JVM_17
		allWarningsAsErrors = true
	}
}

dependencies {
	implementation(libs.androidx.core)
	// Whole, Play Services FIDO and ExoPlayer included: GeckoView loads their classes eagerly and a
	// missing one kills the process (a passkey tap did). Without Google services on the frame,
	// WebAuthn fails cleanly instead.
	implementation(libs.geckoview)

	testImplementation(platform(libs.junit.bom))
	testImplementation(libs.junit.jupiter)
	testImplementation(libs.kotlin.test)
	testRuntimeOnly(libs.junit.launcher)
}
