package app.vitals.phone.service

import app.vitals.core.model.Alert
import app.vitals.core.model.Severity

/**
 * Which alerts one round may announce, and which it must stay quiet about.
 *
 * An alert is announced once, not once per round: the ids announced last
 * round are compared with this round's, so a condition that is still true
 * says nothing new and only a fresh one (or one whose subject changed, which
 * is a different id) reaches the wearer. Without this the five-second poll
 * would buzz every five seconds for as long as the condition lasted.
 *
 * `Info` is never announced at all: the alerts channel is high importance, so
 * posting a note there would make it look and sound like a fault.
 *
 * Pure, and separate from the service, so the rule can be tested without a
 * notification manager (and so the phone notification and the watch vibration
 * are decided from one set rather than two filters that can drift apart).
 */
object AlertDedupe {
    /** The alerts worth a notification: `Info` is a note on screen, not a buzz. */
    fun serious(alerts: List<Alert>): List<Alert> = alerts.filter { it.severity != Severity.Info }

    /**
     * The alerts to announce now, and the id set to remember for the next
     * round. [id] is [Notifications.alertId] bound to the pairing, so an
     * alert's identity is "where, what, on which subject".
     */
    fun fresh(
        previous: Set<Int>,
        alerts: List<Alert>,
        id: (Alert) -> Int,
    ): Pair<List<Pair<Int, Alert>>, Set<Int>> {
        val byId = serious(alerts).associateBy(id)
        return byId.filterKeys { it !in previous }.toList() to byId.keys
    }
}
