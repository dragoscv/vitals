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

@Serializable
enum class ThrottleReason {
    @SerialName("thermal") Thermal,
    @SerialName("powerLimit") PowerLimit,
    @SerialName("currentLimit") CurrentLimit,
    @SerialName("voltageDrop") VoltageDrop,
    @SerialName("powerPolicy") PowerPolicy,
    @SerialName("unknown") Unknown,
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

@Serializable
enum class DiskKind {
    @SerialName("hdd") Hdd,
    @SerialName("ssd") Ssd,
    @SerialName("nvme") Nvme,
    @SerialName("removable") Removable,
    @SerialName("network") Network,
    @SerialName("optical") Optical,
    @SerialName("unknown") Unknown,
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

@Serializable
enum class NetworkKind {
    @SerialName("ethernet") Ethernet,
    @SerialName("wiFi") WiFi,
    @SerialName("cellular") Cellular,
    @SerialName("bluetooth") Bluetooth,
    @SerialName("loopback") Loopback,
    @SerialName("virtual") Virtual,
    @SerialName("vpn") Vpn,
    @SerialName("unknown") Unknown,
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

@Serializable
enum class GpuVendor {
    @SerialName("nvidia") Nvidia,
    @SerialName("amd") Amd,
    @SerialName("intel") Intel,
    @SerialName("apple") Apple,
    @SerialName("qualcomm") Qualcomm,
    @SerialName("unknown") Unknown,
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
) {
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

@Serializable
enum class ProcessKind {
    @SerialName("app") App,
    @SerialName("background") Background,
    @SerialName("service") Service,
    @SerialName("system") System,
    @SerialName("containerized") Containerized,
}

@Serializable
enum class ProcessState {
    @SerialName("running") Running,
    @SerialName("suspended") Suspended,
    @SerialName("waiting") Waiting,
    @SerialName("notResponding") NotResponding,
    @SerialName("zombie") Zombie,
}

@Serializable
enum class IntegrityLevel {
    @SerialName("untrusted") Untrusted,
    @SerialName("low") Low,
    @SerialName("medium") Medium,
    @SerialName("high") High,
    @SerialName("system") System,
    @SerialName("protected") Protected,
}

@Serializable
enum class ProtectionLevel {
    @SerialName("none") None,
    @SerialName("light") Light,
    @SerialName("full") Full,
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

@Serializable
enum class CoreClass {
    @SerialName("performance") Performance,
    @SerialName("efficiency") Efficiency,
    @SerialName("lowPower") LowPower,
    @SerialName("standard") Standard,
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

@Serializable
enum class AlertKind {
    @SerialName("cpuSustained") CpuSustained,
    @SerialName("cpuThrottled") CpuThrottled,
    @SerialName("memoryPressure") MemoryPressure,
    @SerialName("memoryCommit") MemoryCommit,
    @SerialName("diskSaturated") DiskSaturated,
    @SerialName("diskLatency") DiskLatency,
    @SerialName("diskSpace") DiskSpace,
    @SerialName("diskHealth") DiskHealth,
    @SerialName("gpuThrottled") GpuThrottled,
    @SerialName("thermalCpu") ThermalCpu,
    @SerialName("networkErrors") NetworkErrors,
    @SerialName("batteryLow") BatteryLow,
    @SerialName("batteryHealth") BatteryHealth,
}

@Serializable
enum class Severity {
    @SerialName("info") Info,
    @SerialName("warning") Warning,
    @SerialName("critical") Critical,
}

@Serializable
enum class AlertRoute {
    @SerialName("performance") Performance,
    @SerialName("processes") Processes,
    @SerialName("storage") Storage,
    @SerialName("network") Network,
    @SerialName("devices") Devices,
}

/** The only frame shape this client understands (`vitals_core::MODEL_VERSION`). */
const val SUPPORTED_MODEL_VERSION = 1
