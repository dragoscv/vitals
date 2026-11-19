package app.vitals.phone

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * Why a permission needs a matching `<uses-feature>`.
 *
 * Android marks a hardware feature **required** when a permission implies it:
 * `android.permission.CAMERA` implies `android.hardware.camera` and
 * `android.permission.ACCESS_WIFI_STATE` implies `android.hardware.wifi`,
 * unless the manifest declares that feature with `android:required="false"`.
 * Play then hides the app from every device that lacks it — camera-less
 * Chromebooks and Android XR, Ethernet-only phones and tablets — which is a
 * real loss for an app that only ever reads the device it runs on.
 *
 * `android.hardware.camera.any` does **not** cancel what the CAMERA
 * permission implies; the back camera has to be declared optional itself.
 * Getting this wrong is invisible: the source looks right, and only Play's
 * availability report says otherwise.
 *
 * This test pins the source so a later edit cannot quietly re-require a
 * feature. The packaged result is the real proof — `aapt dump badging`, or
 * `scripts/check-android-features.ps1` — because the implied features are
 * added at packaging and never appear in this file; this test catches the
 * mistake before a build is spent on it.
 */
class ManifestFeaturesTest {

    @Test
    fun the_back_camera_the_camera_permission_implies_is_declared_optional() {
        assertOptional("android.hardware.camera")
    }

    @Test
    fun the_wifi_the_access_wifi_state_permission_implies_is_declared_optional() {
        assertOptional("android.hardware.wifi")
    }

    @Test
    fun any_camera_is_declared_optional_beside_the_back_camera() {
        assertOptional("android.hardware.camera.any")
    }

    private fun assertOptional(feature: String) {
        val declared = declaredRequired()
        assertTrue(
            "$feature is not declared in $MANIFEST. A permission implies it, so Android makes " +
                "it REQUIRED and Play hides the app from the devices that lack it.",
            declared.containsKey(feature),
        )
        assertEquals(
            "$feature must carry android:required=\"false\", or Play filters the app from the " +
                "devices that do not have it",
            false,
            declared[feature],
        )
    }

    /** Every `<uses-feature>` in the manifest, by name, with `required` resolved. */
    private fun declaredRequired(): Map<String, Boolean> {
        val factory = DocumentBuilderFactory.newInstance().apply { isNamespaceAware = true }
        val document = factory.newDocumentBuilder().parse(locate())
        // The element is un-namespaced today; "*" keeps this working if the
        // manifest ever gains a default namespace.
        val features = document.getElementsByTagNameNS("*", "uses-feature")
        val result = mutableMapOf<String, Boolean>()
        for (i in 0 until features.length) {
            val element = features.item(i) as? Element ?: continue
            val name = element.getAttributeNS(ANDROID_NS, "name")
            if (name.isEmpty()) continue
            // An absent attribute means required (the platform default).
            val required = element.getAttributeNS(ANDROID_NS, "required")
            result[name] = if (required.isEmpty()) true else required.toBoolean()
        }
        return result
    }

    private fun locate(): File {
        // A Gradle Test task runs with the module directory as its working
        // directory; the second path covers a runner that starts at the root.
        val candidates = listOf("src/main/AndroidManifest.xml", "apps/android/app/src/main/AndroidManifest.xml")
        return candidates.map(::File).firstOrNull(File::isFile)
            ?: error(
                "AndroidManifest.xml not found from ${File(".").absolutePath}; tried ${candidates.joinToString()}",
            )
    }

    private companion object {
        const val MANIFEST = "AndroidManifest.xml"
        const val ANDROID_NS = "http://schemas.android.com/apk/res/android"
    }
}
