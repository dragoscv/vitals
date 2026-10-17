package app.vitals.core.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonClassDiscriminator
import kotlinx.serialization.ExperimentalSerializationApi

// The wire model, mirrored from crates/vitals-core by hand (ADR-0033).
//
// Every nullable field here is `Option<T>` in Rust: null means "not measured",
// never zero, and the UI renders an em dash for it. Do not default a nullable
// field to 0 to make a screen simpler — that is the one rule the whole
// project is built on.
//
// No field has a default value on purpose. The contract test decodes real
// frames with `ignoreUnknownKeys = false` and `explicitNulls = true`, so a
// field that Rust renames, adds or drops fails it rather than silently
// decoding to a default.

@Serializable
data class Frame(
    val seq: Long,
    val timestampMs: Long,
    val elapsedMs: Int,
    val payload: FramePayload,
)

@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("kind")
sealed interface FramePayload {
    val system: SystemMetrics

    @Serializable
    @SerialName("keyframe")
    data class Keyframe(
        override val system: SystemMetrics,
        val processes: List<Process>,
    ) : FramePayload

    @Serializable
    @SerialName("delta")
    data class Delta(
        override val system: SystemMetrics,
        val changed: List<Process>,
        val exited: List<Int>,
    ) : FramePayload
}

@Serializable
data class SystemMetrics(
    val cpu: CpuMetrics,
    val memory: MemoryMetrics,
    val disks: List<DiskMetrics>,
    val networks: List<NetworkMetrics>,
    val gpus: List<GpuMetrics>,
    val powerDraw: Float?,
    val battery: BatteryMetrics?,
    val fans: List<FanMetrics>,
)

@Serializable
data class FanMetrics(val name: String, val rpm: Int)

@Serializable
data class CpuMetrics(
    val total: Float,
    val perCore: List<Float>,
    val kernel: Float,
    val effectiveClock: Long?,
    val maxClock: Long?,
    val temperature: Float?,
    val power: Float?,
    val throttled: ThrottleReason?,
    val processCount: Int,
    val threadCount: Int,
    val handleCount: Int?,
    val uptimeSecs: Long,
    val contextSwitches: Long?,
    val interrupts: Long?,
)

@Serializable(with = ThrottleReason.Serializer::class)
enum class ThrottleReason(override val wire: String) : WireEnum {
    Thermal("thermal"),
    PowerLimit("powerLimit"),
    CurrentLimit("currentLimit"),
    VoltageDrop("voltageDrop"),
    PowerPolicy("powerPolicy"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<ThrottleReason>("ThrottleReason", ThrottleReason.entries.toTypedArray(), ThrottleReason.Unknown)
}

@Serializable
data class MemoryMetrics(
    val total: Long,
    val used: Long,
    val available: Long,
    val cached: Long,
    val pagedPool: Long,
    val nonPagedPool: Long,
    val committed: Long,
    val commitLimit: Long,
    val swapTotal: Long,
    val swapUsed: Long?,
    val hardwareReserved: Long,
    val pageFaultsPerSec: Long?,
    val speed: Long?,
    val slotsUsed: Int?,
    val slotsTotal: Int?,
    val formFactor: String?,
) {
    val usedFraction: Float get() = if (total == 0L) 0f else used.toFloat() / total
}

@Serializable
data class DiskMetrics(
    val id: Int,
    val name: String,
    val model: String?,
    val mount: String?,
    val kind: DiskKind,
    val total: Long,
    val free: Long,
    val read: Long,
    val write: Long,
    val activeTime: Float,
    val responseMs: Float?,
    val queueDepth: Float?,
    val temperature: Float?,
    val health: DiskHealth?,
)

@Serializable(with = DiskKind.Serializer::class)
enum class DiskKind(override val wire: String) : WireEnum {
    Hdd("hdd"),
    Ssd("ssd"),
    Nvme("nvme"),
    Removable("removable"),
    Network("network"),
    Optical("optical"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<DiskKind>("DiskKind", DiskKind.entries.toTypedArray(), DiskKind.Unknown)
}

@Serializable
data class DiskHealth(
    val lifeRemaining: Float?,
    val powerOnHours: Long?,
    val totalWritten: Long?,
    val reallocatedSectors: Long?,
    val failing: Boolean,
)

@Serializable
data class NetworkMetrics(
    val id: Int,
    val name: String,
    val adapter: String?,
    val kind: NetworkKind,
    val rx: Long,
    val tx: Long,
    val rxTotal: Long,
    val txTotal: Long,
    val linkSpeed: Long?,
    val ipv4: String?,
    val ipv6: String?,
    val mac: String?,
    val connected: Boolean,
    val signal: Float?,
    val ssid: String?,
    val errorsPerSec: Long?,
)

@Serializable(with = NetworkKind.Serializer::class)
enum class NetworkKind(override val wire: String) : WireEnum {
    Ethernet("ethernet"),
    WiFi("wiFi"),
    Cellular("cellular"),
    Bluetooth("bluetooth"),
    Loopback("loopback"),
    Virtual("virtual"),
    Vpn("vpn"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<NetworkKind>("NetworkKind", NetworkKind.entries.toTypedArray(), NetworkKind.Unknown)
}

@Serializable
data class GpuMetrics(
    val id: Int,
    val name: String,
    val vendor: GpuVendor,
    val engines: List<GpuEngine>,
    val utilization: Float?,
    val memoryUsed: Long?,
    val memoryTotal: Long?,
    val sharedMemoryUsed: Long?,
    val coreClock: Long?,
    val memoryClock: Long?,
    val temperature: Float?,
    val hotspotTemperature: Float?,
    val power: Float?,
    val powerLimit: Float?,
    val fanPercent: Float?,
    val fanRpm: Int?,
    val throttled: ThrottleReason?,
    val driverVersion: String?,
)

@Serializable
data class GpuEngine(val name: String, val utilization: Float)

@Serializable(with = GpuVendor.Serializer::class)
enum class GpuVendor(override val wire: String) : WireEnum {
    Nvidia("nvidia"),
    Amd("amd"),
    Intel("intel"),
    Apple("apple"),
    Qualcomm("qualcomm"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<GpuVendor>("GpuVendor", GpuVendor.entries.toTypedArray(), GpuVendor.Unknown)
}

@Serializable
data class BatteryMetrics(
    val charge: Float,
    val charging: Boolean,
    val timeRemainingSecs: Long?,
    val power: Float?,
    val health: Float?,
    val cycleCount: Int?,
    val temperature: Float?,
)

@Serializable
data class ProcessKey(val pid: Int, val startTime: Long)

@Serializable
data class Process(
    val key: ProcessKey,
    val parent: Int?,
    val name: String,
    val kind: ProcessKind,
    val state: ProcessState,
    val flags: Int,
    val integrity: IntegrityLevel?,
    val protection: ProtectionLevel,
    val cpu: Float,
    val memoryPrivate: Long,
    val memoryWorkingSet: Long,
    val diskRead: Long,
    val diskWrite: Long,
    val netRx: Long?,
    val netTx: Long?,
    val gpu: Float?,
    val gpuMemory: Long?,
    val threadCount: Int,
    val handleCount: Int?,
    val user: String?,
    val uptimeSecs: Long,
    /** The executable's own name ("Windows Explorer"); null from older desktops. */
    val description: String? = null,
) {
    /** Mirrors `displayName` in `@vitals/protocol`: the app's name, else the file name. */
    val displayName: String
        get() = description?.trim()?.takeIf { it.isNotEmpty() } ?: name

    /** Mirrors `Process::is_safely_terminable` in Rust. */
    val isSafelyTerminable: Boolean
        get() = flags and FLAG_CRITICAL == 0 &&
            protection == ProtectionLevel.None &&
            kind != ProcessKind.System

    val inEfficiencyMode: Boolean get() = flags and FLAG_EFFICIENCY_MODE != 0

    companion object {
        // Bit positions from vitals-core `ProcessFlags`.
        const val FLAG_EFFICIENCY_MODE = 1 shl 3
        const val FLAG_CRITICAL = 1 shl 5
    }
}

@Serializable(with = ProcessKind.Serializer::class)
enum class ProcessKind(override val wire: String) : WireEnum {
    App("app"),
    Background("background"),
    Service("service"),
    System("system"),
    Containerized("containerized"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<ProcessKind>("ProcessKind", ProcessKind.entries.toTypedArray(), ProcessKind.Unknown)
}

@Serializable(with = ProcessState.Serializer::class)
enum class ProcessState(override val wire: String) : WireEnum {
    Running("running"),
    Suspended("suspended"),
    Waiting("waiting"),
    NotResponding("notResponding"),
    Zombie("zombie"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<ProcessState>("ProcessState", ProcessState.entries.toTypedArray(), ProcessState.Unknown)
}

@Serializable(with = IntegrityLevel.Serializer::class)
enum class IntegrityLevel(override val wire: String) : WireEnum {
    Untrusted("untrusted"),
    Low("low"),
    Medium("medium"),
    High("high"),
    System("system"),
    Protected("protected"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<IntegrityLevel>("IntegrityLevel", IntegrityLevel.entries.toTypedArray(), IntegrityLevel.Unknown)
}

@Serializable(with = ProtectionLevel.Serializer::class)
enum class ProtectionLevel(override val wire: String) : WireEnum {
    None("none"),
    Light("light"),
    Full("full"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<ProtectionLevel>("ProtectionLevel", ProtectionLevel.entries.toTypedArray(), ProtectionLevel.Unknown)
}

/** `GET /api/v1/summary`. */
@Serializable
data class Summary(
    val seq: Long,
    val timestampMs: Long,
    val system: SystemMetrics,
    val top: List<Process>,
    val processCount: Int,
)

/** One row of `GET /api/v1/history`. */
@Serializable
data class MachineSample(
    val ts: Long,
    val cpuPercent: Float,
    val cpuKernelPercent: Float,
    val memoryUsed: Long,
    val memoryTotal: Long,
    val diskReadBps: Long,
    val diskWriteBps: Long,
    val netRxBps: Long,
    val netTxBps: Long,
    val gpuPercent: Float?,
    val cpuTempC: Float?,
    val powerDrawW: Float?,
)

/** One row of `GET /api/v1/sensors`. `unit`, `source`, `quality` are translation keys. */
@Serializable
data class SensorLine(
    val key: String,
    val label: String,
    val value: Float,
    val unit: String,
    val source: String,
    val quality: String,
)

@Serializable
data class HostInfo(
    val hostname: String,
    val osName: String,
    val osVersion: String,
    val kernelVersion: String,
    val architecture: String,
    val cpuModel: String,
    val cpuVendor: String,
    val physicalCores: Int,
    val logicalCores: Int,
    val coreTopology: List<CoreClass>?,
    val totalMemory: Long,
    val bootTimeMs: Long,
    val isVirtualMachine: Boolean,
    val motherboard: String?,
    val biosVersion: String?,
)

@Serializable(with = CoreClass.Serializer::class)
enum class CoreClass(override val wire: String) : WireEnum {
    Performance("performance"),
    Efficiency("efficiency"),
    LowPower("lowPower"),
    Standard("standard"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<CoreClass>("CoreClass", CoreClass.entries.toTypedArray(), CoreClass.Unknown)
}

@Serializable
data class Health(val ok: Boolean, val version: String, val modelVersion: Int)

@Serializable
data class Alert(
    val kind: AlertKind,
    val severity: Severity,
    val subject: String,
    val title: String,
    val cause: String,
    /** Untagged in Rust: a number or a string. Kept raw and formatted by the UI. */
    val values: Map<String, kotlinx.serialization.json.JsonPrimitive>,
    val route: AlertRoute?,
    val sinceSample: Long,
)

@Serializable(with = AlertKind.Serializer::class)
enum class AlertKind(override val wire: String) : WireEnum {
    CpuSustained("cpuSustained"),
    CpuThrottled("cpuThrottled"),
    MemoryPressure("memoryPressure"),
    MemoryCommit("memoryCommit"),
    DiskSaturated("diskSaturated"),
    DiskLatency("diskLatency"),
    DiskSpace("diskSpace"),
    DiskHealth("diskHealth"),
    GpuThrottled("gpuThrottled"),
    ThermalCpu("thermalCpu"),
    NetworkErrors("networkErrors"),
    BatteryLow("batteryLow"),
    BatteryHealth("batteryHealth"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<AlertKind>("AlertKind", AlertKind.entries.toTypedArray(), AlertKind.Unknown)
}

@Serializable(with = Severity.Serializer::class)
enum class Severity(override val wire: String) : WireEnum {
    Info("info"),
    Warning("warning"),
    Critical("critical"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<Severity>("Severity", Severity.entries.toTypedArray(), Severity.Unknown)
}

@Serializable(with = AlertRoute.Serializer::class)
enum class AlertRoute(override val wire: String) : WireEnum {
    Performance("performance"),
    Processes("processes"),
    Storage("storage"),
    Network("network"),
    Devices("devices"),
    Unknown("unknown");

    object Serializer : TolerantEnumSerializer<AlertRoute>("AlertRoute", AlertRoute.entries.toTypedArray(), AlertRoute.Unknown)
}

/** The only frame shape this client understands (`vitals_core::MODEL_VERSION`). */
const val SUPPORTED_MODEL_VERSION = 1
