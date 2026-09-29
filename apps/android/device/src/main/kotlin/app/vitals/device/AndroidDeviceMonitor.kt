package app.vitals.device

import android.Manifest
import android.app.AppOpsManager
import android.content.Context
import android.content.pm.PackageManager
import android.hardware.SensorManager
import android.os.Build
import android.os.Environment
import android.os.PowerManager
import android.os.Process
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.vitals.device.internal.AlertRules
import app.vitals.device.internal.AppsReader
import app.vitals.device.internal.BatterySampler
import app.vitals.device.internal.CpuSampler
import app.vitals.device.internal.GpuSampler
import app.vitals.device.internal.HistoryStore
import app.vitals.device.internal.InfoReader
import app.vitals.device.internal.NetworkSampler
import app.vitals.device.internal.SensorsReader
import app.vitals.device.internal.StorageReader
import app.vitals.device.internal.ThermalSampler
import app.vitals.device.model.AppUsage
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.CpuState
import app.vitals.device.model.DeviceAlert
import app.vitals.device.model.DeviceInfo
import app.vitals.device.model.DeviceSample
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.HardwareSensor
import app.vitals.device.model.StorageVolume
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.shareIn
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.concurrent.ConcurrentHashMap

/**
 * The one [DeviceMonitor] implementation. Samplers keep the previous reading
 * to compute deltas (CPU residency, network rate), so every sample goes
 * through one lock and every interval is shared by all its collectors: two
 * screens asking at once get the same readings, not half-second deltas each.
 */
class AndroidDeviceMonitor(context: Context) : DeviceMonitor {
    private val app = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private val lock = Mutex()

    private val cpu = CpuSampler()
    private val gpu = GpuSampler()
    private val thermal = ThermalSampler(app.getSystemService(PowerManager::class.java))
    private val battery = BatterySampler(app)
    private val network = NetworkSampler(app)
    private val infoReader = InfoReader(app, cpu)
    private val storageReader = StorageReader(app)
    private val appsReader = AppsReader(app)
    private val sensors = SensorsReader(app.getSystemService(SensorManager::class.java))
    private val store = HistoryStore(app.filesDir)
    private val rules = AlertRules()

    private val shared = ConcurrentHashMap<Long, Flow<DeviceSnapshot>>()
    private val alertState = MutableStateFlow<List<DeviceAlert>>(emptyList())
    private val accessState = MutableStateFlow(readAccess())
    private var cachedInfo: DeviceInfo? = null
    private var storageFree: Float? = null
    private var storageFreeAt = 0L

    override val alerts: StateFlow<List<DeviceAlert>> = alertState.asStateFlow()
    override val access: StateFlow<Access> = accessState.asStateFlow()
    override val storageRoot: String get() = storageReader.root

    override suspend fun info(): DeviceInfo = cachedInfo ?: withContext(Dispatchers.IO) { infoReader.info() }.also { cachedInfo = it }

    override fun snapshots(intervalMs: Long): Flow<DeviceSnapshot> = shared.getOrPut(intervalMs) {
        flow {
            // Cancellation lands in delay(), so the loop needs no flag of its own.
            while (true) {
                emit(sample())
                delay(intervalMs)
            }
        }
            .flowOn(Dispatchers.IO)
            // Kept for two seconds after the last collector leaves, so a
            // rotation or a tab switch does not throw away the CPU baseline.
            .shareIn(scope, SharingStarted.WhileSubscribed(2_000), replay = 1)
    }

    private suspend fun sample(): DeviceSnapshot = lock.withLock {
        val c = cpu.sample()
        val t = thermal.sample()
        val snapshot = DeviceSnapshot(
            timestampMs = System.currentTimeMillis(),
            cpu = CpuState(
                cores = c.cores,
                clusters = c.clusters,
                load = c.load,
                temperature = t.zones.filter { it.group == "cpu" }.maxOfOrNull { it.celsius },
                measured = c.measured,
            ),
            gpu = gpu.sample(),
            memory = infoReader.memory(),
            battery = battery.sample(),
            thermal = t,
            network = network.sample(),
        )
        alertState.value = rules.evaluate(snapshot, storageFreeFraction(), snapshot.timestampMs)
        snapshot
    }

    /** Refreshed at most once a minute: free space does not move per second, and statfs is not free. */
    private fun storageFreeFraction(): Float? {
        val now = System.currentTimeMillis()
        if (now - storageFreeAt > 60_000) {
            storageFreeAt = now
            val dir = Environment.getDataDirectory()
            storageFree = if (dir.totalSpace > 0) dir.usableSpace.toFloat() / dir.totalSpace else null
        }
        return storageFree
    }

    override suspend fun storage(): List<StorageVolume> = withContext(Dispatchers.IO) {
        storageReader.volumes(accessState.value.usageStats)
    }

    override fun scanFolders(root: String, maxDepth: Int): Flow<ScanProgress> =
        storageReader.scan(root, maxDepth, accessState.value.allFiles).flowOn(Dispatchers.IO)

    override suspend fun cleanupCandidates(): List<CleanupItem> = withContext(Dispatchers.IO) {
        val a = accessState.value
        storageReader.cleanup(a.allFiles, a.usageStats)
    }

    override suspend fun delete(item: CleanupItem): Long? = withContext(Dispatchers.IO) { storageReader.delete(item) }

    override suspend fun apps(windowMs: Long): List<AppUsage> = withContext(Dispatchers.IO) {
        appsReader.read(windowMs, accessState.value.usageStats)
    }

    /**
     * One reading per sensor, all at once: asked in turn, 40 sensors that
     * each wait up to 600 ms for an on-change event kept the list empty for
     * 20 s on the A51.
     */
    override suspend fun hardwareSensors(): List<HardwareSensor> = withContext(Dispatchers.Default) {
        kotlinx.coroutines.coroutineScope {
            sensors.list()
                .map { s -> async { s.copy(values = sensors.once(s.type)) } }
                .awaitAll()
        }
    }

    override fun sensorValues(type: String): Flow<List<Float>> = sensors.values(type)

    override suspend fun history(sinceMs: Long): List<DeviceSample> = withContext(Dispatchers.IO) { store.since(sinceMs) }

    override suspend fun recordSample(): DeviceSample = withContext(Dispatchers.IO) {
        // CPU load is a delta: the first reading only sets the baseline.
        sample()
        delay(1_000)
        sample().toSample().also { store.append(it) }
    }

    override suspend fun record(snapshot: DeviceSnapshot) = withContext(Dispatchers.IO) { store.append(snapshot.toSample()) }

    private fun DeviceSnapshot.toSample() = DeviceSample(
        ts = timestampMs,
        cpuLoad = cpu.load,
        cpuTempC = cpu.temperature,
        gpuLoad = gpu?.load,
        memoryUsedPercent = memory.usedBytes * 100f / memory.totalBytes,
        batteryPercent = battery?.percent,
        batteryTempC = battery?.temperatureC,
        batteryCurrentMa = battery?.currentMa,
        charging = battery?.let { it.status == "charging" || it.status == "full" },
        rxBytesPerSec = network.rxBytesPerSec,
        txBytesPerSec = network.txBytesPerSec,
    )

    override suspend fun pruneHistory(keepMs: Long) = withContext(Dispatchers.IO) { store.prune(keepMs) }

    override fun refreshAccess() {
        accessState.value = readAccess()
    }

    private fun readAccess(): Access = Access(
        usageStats = hasUsageAccess(),
        allFiles = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            ContextCompat.checkSelfPermission(app, Manifest.permission.READ_EXTERNAL_STORAGE) == PackageManager.PERMISSION_GRANTED
        },
        notifications = NotificationManagerCompat.from(app).areNotificationsEnabled(),
    )

    private fun hasUsageAccess(): Boolean {
        val ops = app.getSystemService(AppOpsManager::class.java)
        // Deprecated in SDK 37 with no replacement that works back to minSdk 29;
        // it is still the documented way to read the usage-access toggle.
        @Suppress("DEPRECATION")
        val mode = ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), app.packageName)
        // MODE_DEFAULT defers to the permission, which the Settings toggle grants.
        return if (mode == AppOpsManager.MODE_DEFAULT) {
            app.checkCallingOrSelfPermission(Manifest.permission.PACKAGE_USAGE_STATS) == PackageManager.PERMISSION_GRANTED
        } else {
            mode == AppOpsManager.MODE_ALLOWED
        }
    }
}
