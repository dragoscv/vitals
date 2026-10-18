package app.vitals.core

import android.net.NetworkCapabilities.TRANSPORT_CELLULAR
import android.net.NetworkCapabilities.TRANSPORT_ETHERNET
import android.net.NetworkCapabilities.TRANSPORT_VPN
import android.net.NetworkCapabilities.TRANSPORT_WIFI
import app.vitals.core.net.LanNetwork
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** QA2-8 / AND-6: which networks may carry a pairing token. */
class LanNetworkTest {
    private fun local(vararg transports: Int) = LanNetwork.isLocalTransports { it in transports }

    @Test
    fun wifi_and_ethernet_carry_the_token() {
        assertTrue(local(TRANSPORT_WIFI))
        assertTrue(local(TRANSPORT_ETHERNET))
    }

    @Test
    fun mobile_data_alone_or_no_network_never_does() {
        assertFalse(local(TRANSPORT_CELLULAR))
        assertFalse(local())
    }

    @Test
    fun a_consumer_vpn_over_mobile_data_does_not_get_the_token() {
        // The bug: VPN alone counted as LAN, so 192.168.1.20 inside the
        // provider's network received the bearer token.
        assertFalse(local(TRANSPORT_VPN, TRANSPORT_CELLULAR))
        assertFalse(local(TRANSPORT_VPN))
    }

    @Test
    fun a_vpn_over_wifi_or_ethernet_still_does() {
        assertTrue(local(TRANSPORT_VPN, TRANSPORT_WIFI))
        assertTrue(local(TRANSPORT_VPN, TRANSPORT_ETHERNET))
    }

    @Test
    fun a_vpn_spanning_wifi_and_mobile_data_is_refused_because_either_may_carry_the_request() {
        assertFalse(local(TRANSPORT_VPN, TRANSPORT_WIFI, TRANSPORT_CELLULAR))
    }
}
