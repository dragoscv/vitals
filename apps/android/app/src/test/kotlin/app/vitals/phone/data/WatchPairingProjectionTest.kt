package app.vitals.phone.data

import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * What the phone is allowed to hand the watch (AND-7).
 *
 * A token that can end processes never crosses to the wrist: a watch left on
 * a train must be able to show numbers at most. Getting this wrong is silent
 * — everything keeps working, with too much power — which is exactly why it
 * is asserted rather than trusted.
 */
class WatchPairingProjectionTest {
    private fun pairing(scope: Scope) = Pairing(
        id = "pc1",
        label = "Workstation",
        baseUrl = "http://192.168.1.20:7331",
        token = "secret-token",
        scope = scope,
        createdMs = 1,
    )

    @Test
    fun a_pairing_that_can_control_sends_the_watch_no_token_at_all() {
        assertTrue(pairing(Scope.Control).toWatch().token.isEmpty())
    }

    @Test
    fun a_pairing_whose_scope_is_not_yet_known_sends_no_token_either() {
        // The scope is learnt from a refused control, so "unknown" lasts until
        // then and must be treated as able to control.
        assertTrue(pairing(Scope.Unknown).toWatch().token.isEmpty())
    }

    @Test
    fun only_a_read_only_pairing_carries_its_token_so_the_watch_can_poll_the_pc_itself() {
        assertEquals("secret-token", pairing(Scope.Read).toWatch().token)
    }

    @Test
    fun a_control_pairing_lets_the_watch_ask_the_phone_to_act_while_holding_no_token_itself() {
        val sent = pairing(Scope.Control).toWatch()
        assertEquals("Workstation", sent.label)
        assertEquals("http://192.168.1.20:7331", sent.baseUrl)
        // Control goes through the phone, which holds the token: the watch
        // knows it may ask, and has nothing to leak if it is lost.
        assertTrue(sent.canControl)
        assertTrue(sent.token.isEmpty())
    }
}
