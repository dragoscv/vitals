package app.vitals.device

import app.vitals.device.model.AppUsage
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.DeviceAlert
import app.vitals.device.model.DeviceInfo
import app.vitals.device.model.DeviceSample
import app.vitals.device.model.DeviceSnapshot
import app.vitals.device.model.FolderSize
import app.vitals.device.model.HardwareSensor
import app.vitals.device.model.StorageVolume
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

/**
 * Everything the app knows about the device it runs on. One instance per
 * process. The UI depends on this interface only; the implementation lives
 * beside it and reads official APIs.
 *
 * Cost rule, as on the desktop: nothing is sampled unless a collector asks.
 * [snapshots] is cold and stops when the last collector goes away; the
 * history recorder is the one exception and runs only when the user turns it
 * on, at a slow cadence.
 */
interface DeviceMonitor {
    /** Static facts, read once. */
    suspend fun info(): DeviceInfo

    /** Live readings every [intervalMs] while collected. */
    fun snapshots(intervalMs: Long = 1_000): Flow<DeviceSnapshot>

    suspend fun storage(): List<StorageVolume>

    /** Largest folders under [root], [maxDepth] deep; needs all-files access, else empty. */
    fun scanFolders(root: String, maxDepth: Int = 3): Flow<ScanProgress>

    suspend fun cleanupCandidates(): List<CleanupItem>

    /**
     * Deletes a file-backed cleanup item and returns the bytes freed, or null
     * when it could not be removed. App caches have no [CleanupItem.path]:
     * Android lets only the owning app or the user in Settings clear them.
     */
    suspend fun delete(item: CleanupItem): Long?

    /** Per-app cost over the last [windowMs]. Needs usage access; fields it gates are null without it. */
    suspend fun apps(windowMs: Long = 24 * 3_600_000L): List<AppUsage>

    suspend fun hardwareSensors(): List<HardwareSensor>

    /** Live values of one hardware sensor while collected. */
    fun sensorValues(type: String): Flow<List<Float>>

    /** Samples recorded by the history recorder, oldest first. */
    suspend fun history(sinceMs: Long): List<DeviceSample>

    /**
     * Takes one sample (two readings a second apart, since CPU load is a
     * delta), appends it to history and re-evaluates [alerts]. Called by the
     * periodic worker; the live screen records on its own while collected.
     */
    suspend fun recordSample(): DeviceSample

    /** Appends a snapshot already taken by a live screen, so looking at the phone does not sample it twice. */
    suspend fun record(snapshot: DeviceSnapshot)

    /** Deletes history older than [keepMs]. */
    suspend fun pruneHistory(keepMs: Long = 7 * 24 * 3_600_000L)

    /** Where the folder scan starts: shared storage root. */
    val storageRoot: String

    val alerts: StateFlow<List<DeviceAlert>>

    val access: StateFlow<Access>

    /** Re-reads whether usage and all-files access are granted (after returning from Settings). */
    fun refreshAccess()
}

/** Special permissions, each unlocking a named set of readings. */
data class Access(
    val usageStats: Boolean,
    val allFiles: Boolean,
    val notifications: Boolean,
)

sealed interface ScanProgress {
    data class Scanning(val path: String, val files: Int, val bytes: Long) : ScanProgress
    data class Done(val tree: FolderSize, val elapsedMs: Long) : ScanProgress
    data object NoAccess : ScanProgress
}
