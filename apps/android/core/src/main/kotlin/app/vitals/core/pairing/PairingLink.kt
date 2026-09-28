package app.vitals.core.pairing

import java.net.URI

/**
 * What the desktop's pairing QR code encodes:
 * `http://<ip>:<port>/mobile.html#t=<token>`.
 *
 * The token rides in the URL fragment so a browser never sends it to the
 * server. This parser reads it straight from the scanned text; the URL is
 * never opened, logged or handed to an intent.
 */
data class PairingLink(val baseUrl: String, val token: String) {
    companion object {
        /** A token is 32 random bytes, base64url without padding: always 43 characters. */
        private val TOKEN = Regex("^[A-Za-z0-9_-]{43}$")

        fun parse(text: String): PairingLink? {
            val uri = runCatching { URI(text.trim()) }.getOrNull() ?: return null
            if (uri.scheme != "http" && uri.scheme != "https") return null
            val host = uri.host ?: return null
            val fragment = uri.rawFragment ?: return null
            val token = fragment.split('&')
                .firstNotNullOfOrNull { part ->
                    part.split('=', limit = 2).takeIf { it.size == 2 && it[0] == "t" }?.get(1)
                } ?: return null
            if (!TOKEN.matches(token)) return null
            val port = if (uri.port == -1) 7331 else uri.port
            val hostPart = if (host.contains(':') && !host.startsWith('[')) "[$host]" else host
            return PairingLink("${uri.scheme}://$hostPart:$port", token)
        }
    }
}
