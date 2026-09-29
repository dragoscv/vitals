plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
}

// Google TV: Compose for TV (androidx.tv.material3), no Leanback library.
// Watches this TV through :device and the paired PCs through :core, exactly
// as the phone does, so the numbers on the TV are the phone's numbers.
android {
    namespace = "app.vitals.tv"
    // Compose UI 1.12 and OkHttp 5.5 refuse to link against anything older.
    compileSdk = 37

    defaultConfig {
        // The same id as the phone and the watch: Play ships a TV build as
        // another artefact of one app, on the Android TV form-factor track.
        applicationId = "app.vitals"
        minSdk = 29
        targetSdk = 36
        // Above the watch's range: Play refuses two artefacts of one app with
        // the same versionCode, and a TV must never be offered a phone build.
        versionCode = 2_000_000 + (System.getenv("VITALS_VERSION_CODE")?.toIntOrNull() ?: 1)
        versionName = System.getenv("VITALS_VERSION") ?: "0.0.0-dev"
    }

    signingConfigs {
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

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions { unitTests.isReturnDefaultValues = true }
}

kotlin {
    compilerOptions { allWarningsAsErrors.set(true) }
}

dependencies {
    implementation(project(":core"))
    implementation(project(":device"))
    implementation(project(":shared-ui"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.animation)
    implementation(libs.tv.material)
    implementation(libs.compose.material.icons)
    implementation(libs.activity.compose)
    implementation(libs.core.ktx)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.lifecycle.process)
    implementation(libs.datastore.preferences)
    implementation(libs.work.runtime)
    implementation(libs.profileinstaller)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
}
