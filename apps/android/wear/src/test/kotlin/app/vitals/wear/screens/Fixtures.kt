package app.vitals.wear.screens

import app.vitals.core.WireJson
import app.vitals.core.model.Alert
import app.vitals.core.model.SensorLine
import app.vitals.core.model.Summary
import app.vitals.core.pairing.Pairing
import app.vitals.core.pairing.Scope
import app.vitals.core.wear.PcState
import app.vitals.device.Access
import app.vitals.device.DeviceMonitor
import app.vitals.device.ScanProgress
import app.vitals.device.model.AppUsage
import app.vitals.device.model.BatteryState
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.ClusterState
import app.vitals.device.model.CpuState
import app.vitals.device.model.DeviceAlert
import app.vitals.device.model.DeviceInfo
import app.vitals.device.model.DeviceSample
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.HardwareSensor
import app.vitals.device.model.MemoryState
import app.vitals.device.model.NetworkState
import app.vitals.device.model.StorageVolume
import app.vitals.device.model.ThermalState
import java.io.File
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.serialization.builtins.ListSerializer

/**
 * What Play's reviewer saw, plus the worst cases a real wrist produces: long
 * Windows sensor names, a GPU with a long marketing name, many rows. The PC
 * readings come from the Rust contract fixtures, so the screens render real
 * serialised values rather than a hand-written guess.
 */
internal object Fixtures {
    private fun contract(name: String): String = File("../core/src/test/resources/contract/$name").readText()

    val summary: Summary = WireJson.Lenient.decodeFromString(Summary.serializer(), contract("summary.json"))
    private val alerts: List<Alert> = WireJson.Lenient.decodeFromString(ListSerializer(Alert.serializer()), contract("alerts.json"))

    val sensors: List<SensorLine> = buildList {
        add(SensorLine("cpu.package", "CPU Package (Intel Core i9-14900K)", 81f, "temperature", "kernelDriver", "measured"))
        add(SensorLine("gpu.hotspot", "GPU Hot Spot", 74f, "temperature", "vendorLibrary", "measured"))
        add(SensorLine("nvme0", "Samsung SSD 990 PRO 2TB composite", 48f, "temperature", "storageDevice", "measured"))
        repeat(6) { add(SensorLine("fan$it", "Chassis fan #$it (system board header)", 900f + it * 85, "fanSpeed", "kernelDriver", "measured")) }
        add(SensorLine("cpu.power", "CPU package power", 125.4f, "power", "kernelDriver", "derived"))
        add(SensorLine("vcore", "Vcore", 1.312f, "voltage", "kernelDriver", "measured"))
        add(SensorLine("bat", "Battery wear level", 12f, "percent", "batteryMiniport", "nameplate"))
    }

    val pairing = Pairing(id = "p1", label = "Workstation in the study upstairs", baseUrl = "http://192.168.1.20:7331", token = "", scope = Scope.Read, createdMs = 1)

    val reachable = PcState(
        pairingId = pairing.id,
        label = pairing.label,
        fetchedMs = System.currentTimeMillis(),
        summary = summary,
        sensors = sensors,
        alerts = alerts,
        canControl = false,
    )

    val unreachable = reachable.copy(summary = null, seenMs = System.currentTimeMillis() - 600_000)

    val snapshot = DeviceSnapshot(
        timestampMs = 0,
        cpu = CpuState(
            cores = emptyList(),
            clusters = listOf(
                ClusterState(0, "Efficiency", listOf(0, 1, 2, 3), 300_000_000, 1_800_000_000, 1_100_000_000, 25f),
                ClusterState(1, "Performance", listOf(4), 300_000_000, 2_400_000_000, 2_400_000_000, 60f),
            ),
            load = 25f,
            temperature = 38.5f,
        ),
        gpu = null,
        memory = MemoryState(2_000_000_000, 500_000_000, 1_500_000_000, null, null, null, false, null),
        battery = BatteryState(100f, "full", "wireless", "good", 31f, 4.35f, -120f, null, null, null, null, null),
        thermal = ThermalState("none", null, emptyList()),
        network = NetworkState(null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null),
    )

    val storage = listOf(StorageVolume("internal", "Internal", false, 32_000_000_000, 21_000_000_000, null, null, null, null, null, null))

    /** The reviewer's watch reported 76 sensors with names like these. */
    val hardwareSensors: List<HardwareSensor> = listOf(
        "Gyroscope (wake-up)" to "rad/s",
        "Ambient Light Sensor (non-wakeup)" to "lx",
        "Pressure Sensor Non-wakeup" to "hPa",
        "Accelerometer Uncalibrated" to "m/s²",
        "Heart Rate PPG raw data" to "bpm",
        "Magnetometer (Calibrated, wake-up)" to "µT",
    ).map { (n, u) -> HardwareSensor(n, "Vendor", "t", u, listOf(913.3f), 0.1f, 1000f, 0.1f, false) } +
        List(70) { HardwareSensor("Sensor $it", "Vendor", "t", null, null, 0.1f, 1f, 0.1f, false) }
}

/** A watch that answers instantly and never samples the JVM it runs on. */
internal class FakeMonitor(private val snap: DeviceSnapshot?) : DeviceMonitor {
    override suspend fun info(): DeviceInfo = error("not used")
    override fun snapshots(intervalMs: Long): Flow<DeviceSnapshot> = snap?.let { flowOf(it) } ?: emptyFlow()
    override suspend fun storage(): List<StorageVolume> = Fixtures.storage
    override fun scanFolders(root: String, maxDepth: Int): Flow<ScanProgress> = emptyFlow()
    override suspend fun cleanupCandidates(): List<CleanupItem> = emptyList()
    override suspend fun delete(item: CleanupItem): Long? = null
    override suspend fun apps(windowMs: Long): List<AppUsage> = emptyList()
    override suspend fun hardwareSensors(): List<HardwareSensor> = Fixtures.hardwareSensors
    override fun sensorValues(type: String): Flow<List<Float>> = emptyFlow()
    override suspend fun history(sinceMs: Long): List<DeviceSample> = emptyList()
    override suspend fun recordSample(): DeviceSample = error("not used")
    override suspend fun record(snapshot: DeviceSnapshot) = Unit
    override suspend fun pruneHistory(keepMs: Long) = Unit
    override val storageRoot: String = "/sdcard"
    override val alerts: StateFlow<List<DeviceAlert>> = MutableStateFlow(emptyList())
    override val access: StateFlow<Access> = MutableStateFlow(Access(usageStats = false, allFiles = false, notifications = true))
    override fun refreshAccess() = Unit
}
