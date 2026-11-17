package app.vitals.tv.ui

import app.vitals.core.model.AlertKind
import app.vitals.tv.R
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The TV words every wire value itself, so a value it does not know must land
 * on the generic wording. The alternative — a value silently mapped to a real
 * one — is how a TV ends up claiming something the PC never said.
 */
class WordsTest {
    @Test
    fun an_alert_kind_a_newer_desktop_added_is_worded_generically_rather_than_dropped() {
        assertEquals(R.string.alert_other, AlertKind.Unknown.title())
    }

    @Test
    fun every_known_alert_kind_has_its_own_wording_rather_than_the_generic_one() {
        val generic = R.string.alert_other
        val own = AlertKind.entries.filter { it != AlertKind.Unknown }.map { it.title() }
        assertEquals("no kind may share the generic wording", own.count { it == generic }, 0)
    }

    @Test
    fun a_network_transport_that_is_missing_is_shown_as_disconnected_not_as_a_real_one() {
        assertEquals(R.string.net_disconnected, transport(null))
        assertEquals(R.string.net_disconnected, transport("none"))
    }
}
