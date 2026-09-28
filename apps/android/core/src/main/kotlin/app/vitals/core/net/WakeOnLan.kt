package app.vitals.core.net

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetAddress

/**
 * Wake-on-LAN: six `0xFF` bytes then the MAC sixteen times, as a UDP
 * broadcast on port 9.
 *
 * The one thing the PWA can never do — browsers cannot send raw UDP — and
 * the reason the MAC is copied into the pairing while the PC is awake.
 * Whether the PC wakes depends on its BIOS and adapter settings; the app
 * reports "sent", never "woken", because it cannot know.
 */
object WakeOnLan {
    fun packet(mac: String): ByteArray? {
        val bytes = parseMac(mac) ?: return null
        return ByteArray(6) { 0xFF.toByte() } + ByteArray(16 * 6) { bytes[it % 6] }
    }

    fun parseMac(mac: String): ByteArray? {
        val hex = mac.filter { it.isLetterOrDigit() }
        if (hex.length != 12 || !hex.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) return null
        return ByteArray(6) { hex.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
    }

    /** Sends to [broadcast] (the PC's subnet broadcast when known) and to 255.255.255.255. */
    suspend fun send(mac: String, broadcast: String?): Boolean = withContext(Dispatchers.IO) {
        val payload = packet(mac) ?: return@withContext false
        val targets = listOfNotNull(broadcast, "255.255.255.255").distinct()
        DatagramSocket().use { socket ->
            socket.broadcast = true
            targets.forEach { target ->
                runCatching {
                    val address = InetAddress.getByName(target)
                    repeat(3) { socket.send(DatagramPacket(payload, payload.size, address, 9)) }
                }
            }
        }
        true
    }

    /** `192.168.1.20` -> `192.168.1.255`, assuming a /24 — the common home LAN. */
    fun guessBroadcast(ipv4: String?): String? {
        val parts = ipv4?.split('.')?.takeIf { it.size == 4 } ?: return null
        return "${parts[0]}.${parts[1]}.${parts[2]}.255"
    }
}
