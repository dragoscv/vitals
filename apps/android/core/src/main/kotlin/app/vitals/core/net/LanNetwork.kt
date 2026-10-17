package app.vitals.core.net

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Whether this device is on a network where a paired PC can legitimately be:
 * Wi-Fi or Ethernet, or a VPN the user set up (which carries its own
 * encryption and is how people reach home from outside).
 *
 * Every authenticated request asks this first (AND-6). On mobile data a
 * private address like 192.168.1.20 is not the user's PC: it is whatever the
 * carrier's network routes it to, and the bearer token would be handed to
 * that host. Refusing locally costs nothing — the PC was unreachable there
 * anyway — and lets the screens say "Not on Wi-Fi" instead of a long timeout
 * followed by "can't reach".
 *
 * Until [install] runs (unit tests, a process that never shows UI) the
 * answer is `true`, so nothing that worked before is refused by accident.
 */
object LanNetwork {
    private val installed = AtomicBoolean(false)
    private val _local = MutableStateFlow(true)

    /** `true` on Wi-Fi, Ethernet or a VPN; `false` on mobile data alone or with no network. */
    val local: StateFlow<Boolean> = _local.asStateFlow()

    val onLan: Boolean get() = _local.value

    /** Idempotent; call from `Application.onCreate`. */
    fun install(context: Context) {
        if (!installed.compareAndSet(false, true)) return
        val cm = context.applicationContext.getSystemService(ConnectivityManager::class.java) ?: return
        _local.value = isLocal(cm.getNetworkCapabilities(cm.activeNetwork))
        runCatching {
            cm.registerDefaultNetworkCallback(object : ConnectivityManager.NetworkCallback() {
                override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                    _local.value = isLocal(caps)
                }

                override fun onLost(network: Network) {
                    _local.value = isLocal(cm.getNetworkCapabilities(cm.activeNetwork))
                }
            })
        }
    }

    /** Visible for tests: the transport rule on its own. */
    fun isLocal(caps: NetworkCapabilities?): Boolean {
        caps ?: return false
        return caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) ||
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)
    }
}
