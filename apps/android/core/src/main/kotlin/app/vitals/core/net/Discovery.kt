package app.vitals.core.net

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Build
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import java.util.concurrent.Executors

/** A PC advertising `_vitals._tcp` on this network. */
data class DiscoveredPc(
    val name: String,
    val host: String,
    val port: Int,
    val modelVersion: Int?,
    val version: String?,
) {
    val baseUrl: String get() = "http://${if (host.contains(':')) "[$host]" else host}:$port"
}

/**
 * Finds PCs with Remote access on, over mDNS (ADR-0003: advertised only while
 * the server runs). Holds a multicast lock only while collected — Wi-Fi
 * drivers drop multicast by default to save power, and keeping the lock
 * after the pairing screen closes would cost battery for nothing.
 */
class Discovery(context: Context) {
    private val app = context.applicationContext
    private val nsd = app.getSystemService(NsdManager::class.java)
    /** Null on an Ethernet-only TV box with no Wi-Fi service; discovery still works without the lock. */
    private val wifi: WifiManager? = app.getSystemService(WifiManager::class.java)

    fun scan(): Flow<List<DiscoveredPc>> = callbackFlow {
        // Touched only on [executor]: NSD calls back on its own threads, and
        // a LinkedHashMap mutated from two of them corrupts or throws.
        val found = linkedMapOf<String, DiscoveredPc>()
        val lock = wifi?.createMulticastLock("vitals-discovery")?.apply {
            setReferenceCounted(false)
            acquire()
        }
        val executor = Executors.newSingleThreadExecutor()
        val callbacks = java.util.concurrent.CopyOnWriteArrayList<NsdManager.ServiceInfoCallback>()

        fun publish() = trySend(found.values.toList())

        fun onExecutor(block: () -> Unit) {
            runCatching { executor.execute(block) }
        }

        fun resolved(info: NsdServiceInfo) = onExecutor {
            val host = hostOf(info) ?: return@onExecutor
            val txt = info.attributes
            found[info.serviceName] = DiscoveredPc(
                name = info.serviceName,
                host = host,
                port = info.port,
                modelVersion = txt["model"]?.decodeToString()?.toIntOrNull(),
                version = txt["version"]?.decodeToString(),
            )
            publish()
        }

        fun lost(name: String) = onExecutor {
            found.remove(name)
            publish()
        }

        val listener = object : NsdManager.DiscoveryListener {
            override fun onServiceFound(info: NsdServiceInfo) {
                if (Build.VERSION.SDK_INT >= 34) {
                    val callback = object : NsdManager.ServiceInfoCallback {
                        override fun onServiceUpdated(updated: NsdServiceInfo) = resolved(updated)
                        override fun onServiceLost() = lost(info.serviceName)
                        override fun onServiceInfoCallbackRegistrationFailed(errorCode: Int) = Unit
                        override fun onServiceInfoCallbackUnregistered() = Unit
                    }
                    runCatching { nsd.registerServiceInfoCallback(info, executor, callback) }
                        .onSuccess { callbacks += callback }
                } else {
                    @Suppress("DEPRECATION")
                    nsd.resolveService(info, object : NsdManager.ResolveListener {
                        override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) = Unit
                        override fun onServiceResolved(info: NsdServiceInfo) = resolved(info)
                    })
                }
            }

            override fun onServiceLost(info: NsdServiceInfo) = lost(info.serviceName)

            override fun onDiscoveryStarted(serviceType: String) = Unit
            override fun onDiscoveryStopped(serviceType: String) = Unit
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                close()
            }
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) = Unit
        }

        onExecutor { publish() }
        nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
        awaitClose {
            runCatching { nsd.stopServiceDiscovery(listener) }
            // Each registered callback holds a resolver open in the system
            // until it is unregistered; closing the screen must free them.
            if (Build.VERSION.SDK_INT >= 34) {
                callbacks.forEach { runCatching { nsd.unregisterServiceInfoCallback(it) } }
            }
            executor.shutdown()
            lock?.release()
        }
    }

    private fun hostOf(info: NsdServiceInfo): String? =
        if (Build.VERSION.SDK_INT >= 34) {
            info.hostAddresses.firstOrNull { it is java.net.Inet4Address }?.hostAddress
                ?: info.hostAddresses.firstOrNull()?.hostAddress
        } else {
            @Suppress("DEPRECATION")
            info.host?.hostAddress
        }

    companion object {
        const val SERVICE_TYPE = "_vitals._tcp."
    }
}
