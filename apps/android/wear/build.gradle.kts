plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    // res/raw/aboutlibraries.json for the licences screen (LEG2-11): the watch
    // APK is its own binary and carries its own notices.
    alias(libs.plugins.aboutlibraries)
    // Watch-shape screenshots (S22-12): every screen rendered on round and
    // square watches on the JVM, baselines in src/test/snapshots.
    alias(libs.plugins.roborazzi)
}

android {
    namespace = "app.vitals.wear"
    // Compose UI 1.12 and OkHttp 5.5 refuse to link against anything older;
    // targetSdk stays at the watch's own API level.
    compileSdk = 37

    defaultConfig {
        // The same id as the phone: the Data Layer only delivers between
        // apps that share an application id and a signing key.
        applicationId = "app.vitals"
        minSdk = 30
        targetSdk = 36
        // Play rejects two artefacts of one app with the same versionCode,
        // so the watch build lives in its own range above the phone's.
        versionCode = 1_000_000 + (System.getenv("VITALS_VERSION_CODE")?.toIntOrNull() ?: 1)
        versionName = System.getenv("VITALS_VERSION") ?: "0.0.0-dev"
    }

    signingConfigs {
        // Only in CI or on a release machine; a local release build without
        // the variables is unsigned rather than signed with a stray key.
        if (System.getenv("ANDROID_KEYSTORE_PATH") != null) {
            create("release") {
                storeFile = file(System.getenv("ANDROID_KEYSTORE_PATH"))
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfigs.findByName("release")?.let { signingConfig = it }
        }
    }

    buildFeatures { compose = true }

    // Robolectric needs the merged resources to inflate the Compose screens.
    testOptions.unitTests.isIncludeAndroidResources = true

    // A string missing from values-ro ships English to a Romanian wrist;
    // fail the build instead (AND-46).
    lint {
        error += "MissingTranslation"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions { allWarningsAsErrors.set(true) }
}

roborazzi { outputDir.set(layout.projectDirectory.dir("src/test/snapshots")) }

tasks.withType<Test>().configureEach {
    systemProperty("robolectric.graphicsMode", "NATIVE")
    systemProperty("robolectric.pixelCopyRenderMode", "hardware")
    // Native graphics for a dozen watch-sized renders; the default 512 MB
    // ran out on the largest font scale.
    maxHeapSize = "2g"
}

dependencies {
    implementation(project(":core"))
    implementation(project(":device"))
    implementation(project(":shared-ui"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.core.ktx)

    implementation(libs.wear.compose.material3)
    implementation(libs.wear.compose.foundation)
    implementation(libs.wear.compose.navigation)

    implementation(libs.wear.tiles)
    implementation(libs.wear.protolayout)
    implementation(libs.wear.protolayout.material3)
    implementation(libs.wear.protolayout.expression)
    implementation(libs.wear.complications)

    implementation(libs.play.wearable)
    implementation(libs.coroutines.play.services)
    implementation(libs.coroutines.guava)
    implementation(libs.profileinstaller)
    implementation(libs.aboutlibraries.core)

    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.roborazzi)
    testImplementation(libs.roborazzi.compose)
    testImplementation(libs.compose.ui.test.junit4)
    testImplementation(libs.androidx.test.core)
    testImplementation(libs.androidx.test.junit)
}
