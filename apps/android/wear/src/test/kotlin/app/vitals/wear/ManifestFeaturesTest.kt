package app.vitals.wear

import java.io.File
import javax.xml.parsers.DocumentBuilderFactory
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.w3c.dom.Element

/**
 * The watch half of the phone's `ManifestFeaturesTest`, for the same reason.
 *
 * Android marks a hardware feature REQUIRED when a permission implies it —
 * `ACCESS_WIFI_STATE` implies `android.hardware.wifi` — unless the manifest
 * declares that feature with `android:required="false"`. The watch inherits
 * that permission from `:core`, so the declaration has to be made here or Play
 * hides the watch app from the devices that lack the feature. The implied
 * features never appear in this file (they are added at packaging), so the
 * packaged result is proven by `scripts/check-android-features.ps1`; this
 * catches the mistake before a build is spent on it.
 */
class ManifestFeaturesTest {

    @Test
    fun the_wifi_the_access_wifi_state_permission_implies_is_declared_optional() {
        val declared = declaredRequired()
        assertTrue(
            "android.hardware.wifi is not declared in $MANIFEST. :core requests ACCESS_WIFI_STATE, " +
                "so Android makes it REQUIRED and Play hides the watch from the devices that lack it.",
            declared.containsKey("android.hardware.wifi"),
        )
        assertEquals(
            "android.hardware.wifi must carry android:required=\"false\", or Play filters the watch " +
                "from the devices that do not have it",
            false,
            declared["android.hardware.wifi"],
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
        val candidates = listOf("src/main/AndroidManifest.xml", "apps/android/wear/src/main/AndroidManifest.xml")
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
