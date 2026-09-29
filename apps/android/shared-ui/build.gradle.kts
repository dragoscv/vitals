plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "app.vitals.ui"
    compileSdk = 37
    defaultConfig { minSdk = 29 }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin {
    compilerOptions { allWarningsAsErrors.set(true) }
}

dependencies {
    api(project(":core"))
    api(project(":device"))
    api(platform(libs.compose.bom))
    api(libs.compose.ui.graphics)
    api(libs.compose.runtime)
    implementation(libs.compose.foundation)
    implementation(libs.compose.animation)
    testImplementation(libs.junit)
}
