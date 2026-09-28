package app.vitals.core

import app.vitals.core.model.ControlRequest
import app.vitals.core.model.Priority
import app.vitals.core.model.ProcessKey
import app.vitals.core.net.WakeOnLan
import app.vitals.core.pairing.PairingLink
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PairingAndWolTest {
    private val token = "a".repeat(40) + "-_Z"

    @Test
    fun the_desktop_qr_yields_base_url_and_token_without_the_page_path() {
        val link = PairingLink.parse("http://192.168.1.20:7331/mobile.html#t=$token")!!
        assertEquals("http://192.168.1.20:7331", link.baseUrl)
        assertEquals(token, link.token)
    }

    @Test
    fun anything_that_is_not_a_pairing_link_is_refused() {
        assertNull(PairingLink.parse("http://192.168.1.20:7331/mobile.html"))
        assertNull(PairingLink.parse("http://192.168.1.20:7331/#t=short"))
        assertNull(PairingLink.parse("javascript:alert(1)#t=$token"))
        assertNull(PairingLink.parse("not a url"))
    }

    @Test
    fun a_magic_packet_is_six_ff_then_sixteen_macs() {
        val p = WakeOnLan.packet("AA-BB-CC-DD-EE-FF")!!
        assertEquals(102, p.size)
        assertArrayEquals(ByteArray(6) { 0xFF.toByte() }, p.copyOfRange(0, 6))
        assertEquals(0xAA.toByte(), p[6])
        assertEquals(0xFF.toByte(), p[101])
        assertNull(WakeOnLan.packet("AA-BB-CC"))
        assertNull(WakeOnLan.packet("GG-BB-CC-DD-EE-FF"))
    }

    @Test
    fun control_requests_use_the_servers_kebab_case_tags() {
        val json = WireJson.Lenient.encodeToString(
            ControlRequest.serializer(),
            ControlRequest.SetPriority(ProcessKey(4, 5), Priority.BelowNormal),
        )
        assertEquals("""{"action":"set-priority","key":{"pid":4,"startTime":5},"priority":"below-normal"}""", json)
    }

    @Test
    fun a_token_is_only_ever_sent_to_a_private_address() {
        listOf(
            "http://192.168.1.20:7331", "http://10.0.0.5:7331", "http://172.20.1.1:7331",
            "http://100.101.1.2:7331", "http://desk.local:7331", "http://[fe80::1]:7331",
        ).forEach { org.junit.Assert.assertTrue(it, app.vitals.core.net.LanAddress.isPrivate(it)) }
        listOf(
            "http://8.8.8.8:7331", "http://172.32.0.1:7331", "http://evil.example:7331", "http://[2001:db8::1]:7331",
        ).forEach { org.junit.Assert.assertFalse(it, app.vitals.core.net.LanAddress.isPrivate(it)) }
    }
}
