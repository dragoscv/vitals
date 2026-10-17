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
 *
 * Hostnames: only `localhost` and mDNS `.local` names are accepted, because
 * a `.local` name is resolved on the link by multicast and never by a public
 * DNS server. Discovery itself hands over numeric addresses (the resolved
 * `hostAddress`), so `.local` matters only for an address typed by hand.
 * Every other name is refused even if it would resolve to a private IP —
 * the name's owner, not the user, decides what it resolves to.
 */
object LanAddress {
    fun isPrivate(baseUrl: String): Boolean {
        val host = runCatching { URI(baseUrl).host }.getOrNull()?.trim('[', ']') ?: return false
        if (host.endsWith(".local", ignoreCase = true) || host.equals("localhost", ignoreCase = true)) return true
        return isPrivateIpv4(host) || isPrivateIpv6(host)
    }

    /**
     * Exactly four decimal octets, each 0–255. The first version kept only
     * the parts that parsed as numbers, so `10.0.0.1.evil.com` (a public
     * name) and `10.0.0.256.1` both reduced to four octets and passed.
     */
    private fun isPrivateIpv4(host: String): Boolean {
        val parts = host.split('.')
        if (parts.size != 4) return false
        if (parts.any { it.isEmpty() || it.length > 3 || !it.all(Char::isDigit) }) return false
        val p = parts.map { it.toInt() }
        if (p.any { it > 255 }) return false
        return p[0] == 10 ||
            (p[0] == 172 && p[1] in 16..31) ||
            (p[0] == 192 && p[1] == 168) ||
            (p[0] == 169 && p[1] == 254) ||
            p[0] == 127 ||
            // CGNAT: Tailscale and some ISP-provided home routers.
            (p[0] == 100 && p[1] in 64..127)
    }

    /** Loopback, link-local `fe80::/10` and unique-local `fc00::/7`, judged on the first hextet as a number. */
    private fun isPrivateIpv6(host: String): Boolean {
        // A link-local address carries its interface after `%` (fe80::1%wlan0).
        val h = host.lowercase().substringBefore('%')
        if (!h.contains(':') || !h.all { it in '0'..'9' || it in 'a'..'f' || it == ':' || it == '.' }) return false
        if (h == "::1") return true
        val first = h.substringBefore(':')
        if (first.isEmpty() || first.length > 4) return false
        val n = first.toInt(16)
        return (n and 0xffc0) == 0xfe80 || (n and 0xfe00) == 0xfc00
    }
}
