// AGP 9 compiles Kotlin itself (built-in Kotlin), so there is no
// org.jetbrains.kotlin.android plugin here. The Kotlin Gradle plugin is put
// on the build classpath at the catalogue version so the compiler, the
// Compose compiler plugin and the serialization plugin all agree on 2.4.20
// rather than on whatever AGP bundles.
buildscript {
    dependencies {
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:${libs.versions.kotlin.get()}")
    }
}

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.android.library) apply false
    alias(libs.plugins.android.test) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
    alias(libs.plugins.baselineprofile) apply false
}
