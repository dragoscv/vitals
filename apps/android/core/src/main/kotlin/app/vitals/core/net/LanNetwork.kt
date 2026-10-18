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
 * Wi-Fi or Ethernet, or a VPN that itself runs over Wi-Fi or Ethernet.
 *
 * Every authenticated request asks this first (AND-6). On mobile data a
 * private address like 192.168.1.20 is not the user's PC: it is whatever the
 * carrier's network routes it to, and the bearer token would be handed to
 * that host. Refusing locally costs nothing — the PC was unreachable there
 * anyway — and lets the screens say "Not on Wi-Fi" instead of a long timeout
 * followed by "can't reach".
 *
 * A VPN is not trusted for being a VPN (QA2-8). A consumer VPN on mobile data
 * routes 192.168.1.20 to whatever sits at that address inside the provider's
 * network, which is exactly the stranger this guard exists for; only a VPN
 * riding on Wi-Fi or Ethernet keeps the token on a network the user is
 * standing in (or tunnels home from one). Android merges the transports of a
 * VPN's underlying networks into the VPN's own capabilities, so a VPN over
 * Wi-Fi reports `VPN|WIFI` and over mobile data `VPN|CELLULAR`. The precise
 * API, `NetworkCapabilities.getUnderlyingNetworks()`, is a system API an app
 * cannot call, so the rule reads those merged transports and is conservative:
 * a VPN with no Wi-Fi or Ethernet underneath, or with mobile data among its
 * underlying networks (a split tunnel can send the request either way), is
 * refused. Someone reaching home over mobile data through their own VPN loses
 * that; handing the token to an unknown host is the worse failure.
 *
 * Until [install] runs (unit tests, a process that never shows UI) the
 * answer is `true`, so nothing that worked before is refused by accident.
 */
object LanNetwork {
    private val installed = AtomicBoolean(false)
    private val _local = MutableStateFlow(true)

    /** `true` on Wi-Fi, Ethernet or a VPN over one of them; `false` on mobile data or with no network. */
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

    /** The transport rule applied to a real network's capabilities. */
    fun isLocal(caps: NetworkCapabilities?): Boolean {
        caps ?: return false
        return isLocalTransports(caps::hasTransport)
    }

    /**
     * The rule on its own, over "does the network have this transport", so it
     * can be tested on the JVM where [NetworkCapabilities] is a stub.
     */
    fun isLocalTransports(has: (Int) -> Boolean): Boolean {
        val lan = has(NetworkCapabilities.TRANSPORT_WIFI) || has(NetworkCapabilities.TRANSPORT_ETHERNET)
        if (!has(NetworkCapabilities.TRANSPORT_VPN)) return lan
        return lan && !has(NetworkCapabilities.TRANSPORT_CELLULAR)
    }
}
