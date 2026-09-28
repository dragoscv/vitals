package app.vitals.device.internal

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.TrafficStats
import android.net.wifi.ScanResult
import android.net.wifi.WifiInfo
import android.os.Build
import android.os.SystemClock
import android.telephony.CellSignalStrengthGsm
import android.telephony.CellSignalStrengthLte
import android.telephony.CellSignalStrengthNr
import android.telephony.CellSignalStrengthWcdma
import android.telephony.TelephonyManager
import app.vitals.device.model.NetworkState
import java.net.Inet4Address
import java.net.Inet6Address

/**
 * Throughput from TrafficStats (device totals, still open to apps where
 * `/proc/net/dev` is not) and link facts from ConnectivityManager. Nothing
 * here needs location or phone-state permission: the SSID and the cell type
 * that those gate are left out rather than asked for.
 */
internal class NetworkSampler(context: Context) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val telephony = context.getSystemService(TelephonyManager::class.java)
    private var lastRx = -1L
    private var lastTx = -1L
    private var lastAt = 0L

    fun sample(): NetworkState {
        val rx = TrafficStats.getTotalRxBytes().takeIf { it != TrafficStats.UNSUPPORTED.toLong() }
        val tx = TrafficStats.getTotalTxBytes().takeIf { it != TrafficStats.UNSUPPORTED.toLong() }
        val now = SystemClock.elapsedRealtime()
        val dt = (now - lastAt) / 1000.0
        val rxRate = rate(rx, lastRx, dt)
        val txRate = rate(tx, lastTx, dt)
        lastRx = rx ?: -1
        lastTx = tx ?: -1
        lastAt = now

        val network = connectivity?.activeNetwork
        val caps = network?.let { connectivity.getNetworkCapabilities(it) }
        val links = network?.let { connectivity.getLinkProperties(it) }
        val transport = when {
            caps == null -> "none"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
            else -> "other"
        }
        val wifi = caps?.transportInfo as? WifiInfo
        val cellular = transport == "cellular"
        val addresses = links?.linkAddresses.orEmpty().map { it.address }
        return NetworkState(
            transport = transport,
            rxBytesPerSec = rxRate,
            txBytesPerSec = txRate,
            rxTotalBytes = rx,
            txTotalBytes = tx,
            downstreamKbps = caps?.linkDownstreamBandwidthKbps?.takeIf { it > 0 },
            upstreamKbps = caps?.linkUpstreamBandwidthKbps?.takeIf { it > 0 },
            metered = caps?.let { !it.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) },
            validated = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED),
            wifiRssiDbm = wifi?.rssi?.takeIf { it in -127..0 },
            wifiLinkMbps = wifi?.linkSpeed?.takeIf { it > 0 },
            wifiFrequencyMhz = wifi?.frequency?.takeIf { it > 0 },
            wifiStandard = wifi?.let(::standard),
            cellularGeneration = if (cellular) generation() else null,
            cellularSignalDbm = if (cellular) cellDbm() else null,
            operator = if (cellular) telephony?.networkOperatorName?.takeIf { it.isNotBlank() } else null,
            ipv4 = addresses.firstOrNull { it is Inet4Address }?.hostAddress,
            ipv6 = addresses.firstOrNull { it is Inet6Address && !it.isLinkLocalAddress }?.hostAddress?.substringBefore('%'),
        )
    }

    /** Null on the first sample and whenever a counter went backwards (reset on network change). */
    private fun rate(now: Long?, before: Long, seconds: Double): Long? {
        if (now == null || before < 0 || seconds <= 0.2) return null
        val delta = now - before
        return if (delta < 0) null else (delta / seconds).toLong()
    }

    private fun standard(info: WifiInfo): String? {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return null
        return when (info.wifiStandard) {
            ScanResult.WIFI_STANDARD_LEGACY -> "802.11a/b/g"
            ScanResult.WIFI_STANDARD_11N -> "Wi-Fi 4"
            ScanResult.WIFI_STANDARD_11AC -> "Wi-Fi 5"
            ScanResult.WIFI_STANDARD_11AX -> "Wi-Fi 6"
            ScanResult.WIFI_STANDARD_11BE -> "Wi-Fi 7"
            else -> null
        }
    }

    /** From the signal-strength object's class, which needs no permission (the network type does). */
    private fun generation(): String? {
        val strengths = telephony?.signalStrength?.cellSignalStrengths.orEmpty()
        return when {
            strengths.any { it is CellSignalStrengthNr } -> "5G"
            strengths.any { it is CellSignalStrengthLte } -> "4G"
            strengths.any { it is CellSignalStrengthWcdma } -> "3G"
            strengths.any { it is CellSignalStrengthGsm } -> "2G"
            else -> null
        }
    }

    private fun cellDbm(): Int? = telephony?.signalStrength?.cellSignalStrengths
        ?.map { it.dbm }
        ?.firstOrNull { it in -140..-20 }
}
