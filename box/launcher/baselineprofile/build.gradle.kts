// Baseline profile producer for :app, on a real box (API 33+, without root):
//
//   ./gradlew :app:generateReleaseBaselineProfile \
//     -Pandroid.injected.androidTest.leaveApksInstalledAfterRun=true
//
// The property matters: the app is the box's only home, and a test run uninstalls what it tested.
// The plugin writes app/src/release/generated/baselineProfiles/baseline-prof.txt (commit it);
// profileinstaller ships it in the sideloaded APK.
plugins {
	alias(libs.plugins.android.test)
	alias(libs.plugins.baselineprofile)
}

android {
	namespace = "maison.box.baselineprofile"
	compileSdk = 37

	defaultConfig {
		minSdk = 29
		targetSdk = 37
		testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
	}

	compileOptions {
		sourceCompatibility = JavaVersion.VERSION_17
		targetCompatibility = JavaVersion.VERSION_17
	}

	targetProjectPath = ":app"
}

baselineProfile {
	useConnectedDevices = true
}

dependencies {
	implementation(libs.androidx.test.ext.junit)
	implementation(libs.uiautomator)
	implementation(libs.benchmark.macro.junit4)
}
