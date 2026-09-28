package app.vitals.core.net

import java.net.URI

/**
 * Whether a PC address is on a private network.
 *
 * The LAN API is plain HTTP by design (ADR-0003), so the apps must permit
 * cleartext. Android's network security config cannot express IP ranges,
 * only domain names, so the restriction lives here instead: every pairing
 * and every discovered PC is checked, and a public address is refused. A
 * QR code that points a bearer token at the internet is refused rather than
 * obeyed.
 */
object LanAddress {
    fun isPrivate(baseUrl: String): Boolean {
        val host = runCatching { URI(baseUrl).host }.getOrNull()?.trim('[', ']') ?: return false
        if (host.endsWith(".local", ignoreCase = true) || host.equals("localhost", ignoreCase = true)) return true
        return isPrivateIpv4(host) || isPrivateIpv6(host)
    }

    private fun isPrivateIpv4(host: String): Boolean {
        val p = host.split('.').mapNotNull { it.toIntOrNull()?.takeIf { n -> n in 0..255 } }
        if (p.size != 4) return false
        return p[0] == 10 ||
            (p[0] == 172 && p[1] in 16..31) ||
            (p[0] == 192 && p[1] == 168) ||
            (p[0] == 169 && p[1] == 254) ||
            p[0] == 127 ||
            // CGNAT: Tailscale and some ISP-provided home routers.
            (p[0] == 100 && p[1] in 64..127)
    }

    private fun isPrivateIpv6(host: String): Boolean {
        val h = host.lowercase()
        if (!h.contains(':')) return false
        return h == "::1" || h.startsWith("fe8") || h.startsWith("fe9") || h.startsWith("fea") ||
            h.startsWith("feb") || h.startsWith("fc") || h.startsWith("fd")
    }
}
