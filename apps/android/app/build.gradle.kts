plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Release signing comes only from the environment (CI secrets or a local
// shell), so no keystore path or password ever lands in the repository.
// Without ANDROID_KEYSTORE_PATH the release build is simply unsigned.
val keystorePath: String? = System.getenv("ANDROID_KEYSTORE_PATH")

android {
    namespace = "app.vitals.phone"
    // 37, not 36: Compose 1.12, Navigation 3 1.2 and Haze 2.0 refuse to be
    // compiled against anything older (checkAarMetadata). targetSdk stays 36,
    // so runtime behaviour does not change.
    compileSdk = 37

    defaultConfig {
        // Shared with :wear — the Wearable Data Layer only connects apps with
        // the same application id and signing key.
        applicationId = "app.vitals"
        minSdk = 29
        targetSdk = 36
        // The release workflow passes the tag's version and the run number;
        // a local build is 0.0.0-dev and never mistaken for a release.
        versionCode = System.getenv("VITALS_VERSION_CODE")?.toIntOrNull() ?: 1
        versionName = System.getenv("VITALS_VERSION") ?: "0.0.0-dev"
        // ML Kit's native scanner ships for four ABIs; x86 and x86_64 were
        // 11.6 MB of a 26.7 MB APK and only ever run on an emulator. Every
        // phone this is for is ARM. Remove this to debug on an x86 emulator.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
    }

    signingConfigs {
        if (keystorePath != null) {
            create("release") {
                storeFile = file(keystorePath)
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
            if (keystorePath != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions {
        allWarningsAsErrors.set(true)
    }
}

dependencies {
    implementation(project(":core"))
    implementation(project(":device"))
    implementation(project(":shared-ui"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.animation)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)

    implementation(libs.activity.compose)
    implementation(libs.core.ktx)
    implementation(libs.core.splashscreen)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.service)
    implementation(libs.lifecycle.process)
    implementation(libs.navigation3.runtime)
    implementation(libs.navigation3.ui)
    implementation(libs.lifecycle.viewmodel.navigation3)
    implementation(libs.datastore.preferences)
    implementation(libs.work.runtime)
    implementation(libs.glance.appwidget)
    implementation(libs.glance.material3)

    implementation(libs.haze)
    implementation(libs.haze.blur)

    implementation(libs.camerax.camera2)
    implementation(libs.camerax.lifecycle)
    implementation(libs.camerax.compose)
    implementation(libs.mlkit.barcode)

    implementation(libs.play.wearable)
    implementation(libs.coroutines.play.services)
    implementation(libs.profileinstaller)
}
