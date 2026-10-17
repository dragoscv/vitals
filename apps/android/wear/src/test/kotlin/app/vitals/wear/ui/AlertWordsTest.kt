package app.vitals.wear.ui

import app.vitals.core.model.Alert
import app.vitals.core.model.AlertKind
import app.vitals.core.model.Severity
import app.vitals.wear.R
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.io.File
import javax.xml.parsers.DocumentBuilderFactory

/**
 * The desktop's alert `title`/`cause` are its own i18n keys. The watch once
 * showed them raw ("alert.cpu_hot.title"); these tests resolve what the
 * watch would really display, in both locales, from the shipped resources.
 */
class AlertWordsTest {
    private fun strings(dir: String): Map<String, String> {
        val file = File("src/main/res/$dir/strings.xml")
        val doc = DocumentBuilderFactory.newInstance().newDocumentBuilder().parse(file)
        val nodes = doc.getElementsByTagName("string")
        return (0 until nodes.length).associate { i ->
            val n = nodes.item(i)
            n.attributes.getNamedItem("name").nodeValue to n.textContent
        }
    }

    /** R.string id back to its name, so the resolver reads the real XML rather than a hand-kept copy. */
    private val names: Map<Int, String> = R.string::class.java.fields.associate { it.getInt(null) to it.name }

    private fun wireAlert(kind: AlertKind, subject: String = "C:") = Alert(
        kind = kind,
        severity = Severity.Critical,
        subject = subject,
        title = "alert.${kind.name}.title",
        cause = "alert.${kind.name}.cause",
        values = emptyMap(),
        route = null,
        sinceSample = 0,
    )

    @Test
    fun no_alert_kind_is_shown_as_a_wire_key_in_either_language() {
        for (locale in listOf("values", "values-ro")) {
            val table = strings(locale)
            for (kind in AlertKind.entries) {
                val text = alertText(wireAlert(kind)) { id ->
                    val name = names[id] ?: error("unknown string id $id for $kind")
                    table[name] ?: error("$locale is missing $name for $kind")
                }
                assertFalse("$locale/$kind title: ${text.title}", text.title.startsWith("alert."))
                assertFalse("$locale/$kind body: ${text.body}", text.body.orEmpty().startsWith("alert."))
                assertFalse("$locale/$kind title is blank", text.title.isBlank())
            }
        }
    }

    @Test
    fun the_body_is_the_subject_and_absent_when_the_pc_names_none() {
        val resolve: (Int) -> String = { "worded" }
        assertEquals("C:", alertText(wireAlert(AlertKind.DiskSpace), resolve).body)
        assertEquals(null, alertText(wireAlert(AlertKind.CpuSustained, subject = ""), resolve).body)
    }
}
