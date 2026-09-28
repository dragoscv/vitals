plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.serialization)
}

// The device this app runs on, read through official APIs only (ADR-0035):
// no root, no Shizuku. Used by the phone and the watch; each reading that the
// platform refuses is null, never zero.
android {
    namespace = "app.vitals.device"
    compileSdk = 37
    defaultConfig { minSdk = 29 }
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
    api(libs.coroutines.android)
    api(libs.serialization.json)
    implementation(libs.core.ktx)

    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
}
