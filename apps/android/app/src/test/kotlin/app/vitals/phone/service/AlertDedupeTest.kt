package app.vitals.phone.service

import app.vitals.core.model.Alert
import app.vitals.core.model.AlertKind
import app.vitals.core.model.Severity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** One condition on a PC, worded the way `GET /api/v1/alerts` words it. */
private fun condition(kind: AlertKind, severity: Severity, subject: String) = Alert(
    kind = kind,
    severity = severity,
    subject = subject,
    title = "",
    cause = "",
    values = emptyMap(),
    route = null,
    sinceSample = 0,
)

/**
 * The rule that keeps one alert one buzz. The service polls every five
 * seconds, so anything this gets wrong is felt five seconds later, for as
 * long as the condition lasts.
 */
class AlertDedupeTest {
    private val cpu = condition(AlertKind.CpuSustained, Severity.Warning, "cpu.package")
    private val diskC = condition(AlertKind.DiskSpace, Severity.Critical, "C:")
    private val id = { alert: Alert -> Notifications.alertId("pc1", alert) }

    @Test
    fun an_alert_that_is_still_the_same_condition_is_announced_once_not_once_per_round() {
        val (first, remembered) = AlertDedupe.fresh(emptySet(), listOf(cpu), id)
        assertEquals(1, first.size)
        val (second, _) = AlertDedupe.fresh(remembered, listOf(cpu), id)
        assertTrue("an unchanged alert was announced again: $second", second.isEmpty())
    }

    @Test
    fun a_quiet_round_still_remembers_what_is_standing_so_it_is_not_treated_as_new_next_time() {
        val (_, remembered) = AlertDedupe.fresh(emptySet(), listOf(cpu), id)
        val (quiet, stillRemembered) = AlertDedupe.fresh(remembered, listOf(cpu), id)
        assertTrue(quiet.isEmpty())
        assertEquals(remembered, stillRemembered)
    }

    @Test
    fun an_informational_alert_never_reaches_the_notification_or_the_watch() {
        val note = condition(AlertKind.CpuSustained, Severity.Info, "cpu.package")
        val (fresh, remembered) = AlertDedupe.fresh(emptySet(), listOf(note), id)
        assertTrue("an info alert was announced: $fresh", fresh.isEmpty())
        assertTrue("an info alert was remembered as standing: $remembered", remembered.isEmpty())
    }

    @Test
    fun two_alerts_of_the_same_kind_on_different_subjects_are_both_announced() {
        val diskD = condition(AlertKind.DiskSpace, Severity.Critical, "D:")
        val (fresh, _) = AlertDedupe.fresh(emptySet(), listOf(diskC, diskD), id)
        assertEquals("two disks full is two alerts", 2, fresh.size)
    }

    @Test
    fun an_alert_that_clears_and_comes_back_is_announced_again() {
        val (_, remembered) = AlertDedupe.fresh(emptySet(), listOf(diskC), id)
        val (cleared, empty) = AlertDedupe.fresh(remembered, emptyList(), id)
        assertTrue(cleared.isEmpty())
        assertTrue(empty.isEmpty())
        // The disk filled up again later: that is news, not a repeat.
        val (again, _) = AlertDedupe.fresh(empty, listOf(diskC), id)
        assertEquals(1, again.size)
    }

    @Test
    fun the_same_condition_on_two_pcs_is_two_alerts() {
        val (fresh, _) = AlertDedupe.fresh(emptySet(), listOf(diskC)) { Notifications.alertId("pc2", it) }
        assertEquals(1, fresh.size)
        // The id carries the pairing, so the other PC's identical alert is a
        // different notification rather than a silent overwrite.
        assertTrue(Notifications.alertId("pc1", diskC) != Notifications.alertId("pc2", diskC))
    }
}
