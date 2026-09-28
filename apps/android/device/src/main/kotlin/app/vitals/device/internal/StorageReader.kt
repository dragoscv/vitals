package app.vitals.device.internal

import android.app.usage.StorageStatsManager
import android.content.Context
import android.os.Environment
import android.os.Process
import android.os.storage.StorageManager
import app.vitals.device.ScanProgress
import app.vitals.device.model.CleanupItem
import app.vitals.device.model.FolderSize
import app.vitals.device.model.StorageVolume
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.yield
import java.io.File
import java.util.UUID

/**
 * Volumes, the folder scan and cleanup candidates. The scan walks shared
 * storage with plain File APIs, which needs all-files access
 * (MANAGE_EXTERNAL_STORAGE); without it an app sees only its own folders and
 * the result would be a confident lie, so it reports [ScanProgress.NoAccess].
 */
internal class StorageReader(private val context: Context) {
    private val storage = context.getSystemService(StorageManager::class.java)
    private val stats = context.getSystemService(StorageStatsManager::class.java)

    val root: String = Environment.getExternalStorageDirectory().path

    fun volumes(usageAccess: Boolean): List<StorageVolume> = storage.storageVolumes.mapNotNull { v ->
        val dir = directoryOf(v) ?: return@mapNotNull null
        val uuid: UUID = runCatching { storage.getUuidForPath(dir) }.getOrNull() ?: return@mapNotNull null
        val total = runCatching { stats.getTotalBytes(uuid) }.getOrNull() ?: dir.totalSpace
        val free = runCatching { stats.getFreeBytes(uuid) }.getOrNull() ?: dir.usableSpace
        // Categories come from the media store's own accounting, queried per
        // user; they need usage access and are only defined for the primary volume.
        val ext = if (usageAccess && v.isPrimary) {
            runCatching { stats.queryExternalStatsForUser(uuid, Process.myUserHandle()) }.getOrNull()
        } else {
            null
        }
        val apps = if (usageAccess && v.isPrimary) appsBytes(uuid, ext?.totalBytes) else null
        val used = total - free
        val known = listOfNotNull(apps, ext?.totalBytes)
        StorageVolume(
            id = v.uuid ?: "primary",
            label = v.getDescription(context),
            removable = v.isRemovable,
            totalBytes = total,
            freeBytes = free,
            apps = apps,
            images = ext?.imageBytes,
            video = ext?.videoBytes,
            audio = ext?.audioBytes,
            otherFiles = ext?.let { (it.totalBytes - it.imageBytes - it.videoBytes - it.audioBytes).coerceAtLeast(0) },
            // The two sources overlap (app folders under Android/ count in
            // both), so on a full phone they can exceed "used": the S25 read
            // 230 + 104 GB of 287 GB. A remainder of zero would claim the OS
            // takes no space, so an impossible sum is unknown, not 0.
            system = if (known.size == 2) (used - known.sum()).takeIf { it > 0 } else null,
        )
    }

    /** StorageVolume.directory is API 30; before that only the primary volume has a public path. */
    private fun directoryOf(v: android.os.storage.StorageVolume): File? =
        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
            v.directory
        } else if (v.isPrimary) {
            Environment.getExternalStorageDirectory()
        } else {
            null
        }

    /**
     * Every app's code + data + cache for this user in one query (summing per
     * package would miss every app hidden by package visibility). The user
     * total counts shared storage too, because /sdcard lives on the same
     * partition; subtracting it is what makes the categories add up. On the
     * S25 Ultra this gives 199 GB against Settings' 202 GB (dumpsys
     * diskstats: app 82.8 + data 93.7 + cache 25.1), where the raw total
     * claimed 334 GB of a 287 GB disk.
     */
    private fun appsBytes(uuid: UUID, sharedBytes: Long?): Long? = runCatching {
        val s = stats.queryStatsForUser(uuid, Process.myUserHandle())
        (s.appBytes + s.dataBytes - (sharedBytes ?: 0L)).coerceAtLeast(0)
    }.getOrNull()

    fun scan(rootPath: String, maxDepth: Int, allFiles: Boolean): Flow<ScanProgress> = flow {
        if (!allFiles) {
            emit(ScanProgress.NoAccess)
            return@flow
        }
        val started = System.currentTimeMillis()
        val counter = Counter()
        val tree = walk(File(rootPath), 0, maxDepth, counter) { path ->
            emit(ScanProgress.Scanning(path, counter.files, counter.bytes))
        }
        emit(ScanProgress.Done(tree, System.currentTimeMillis() - started))
    }

    private class Counter {
        var files = 0
        var bytes = 0L
        var lastEmit = 0L
    }

    /**
     * Depth-first, keeping children only down to [maxDepth] but counting
     * everything below it, so a folder's size is its real size. Symlinks are
     * not followed: Android's /sdcard has loops through /storage/emulated.
     */
    private suspend fun walk(
        dir: File,
        depth: Int,
        maxDepth: Int,
        counter: Counter,
        progress: suspend (String) -> Unit,
    ): FolderSize {
        var bytes = 0L
        var files = 0
        val children = ArrayList<FolderSize>()
        val entries = dir.listFiles().orEmpty()
        for (f in entries) {
            if (isLink(f)) continue
            if (f.isDirectory) {
                val child = walk(f, depth + 1, maxDepth, counter, progress)
                bytes += child.bytes
                files += child.files
                if (depth < maxDepth) children += child
            } else {
                val len = f.length()
                bytes += len
                files += 1
                counter.files += 1
                counter.bytes += len
            }
        }
        val now = System.currentTimeMillis()
        if (now - counter.lastEmit > 150) {
            counter.lastEmit = now
            progress(dir.path)
            yield()
        }
        children.sortByDescending { it.bytes }
        return FolderSize(dir.path, dir.name.ifEmpty { dir.path }, bytes, files, children.take(40))
    }

    private fun isLink(f: File): Boolean = runCatching {
        java.nio.file.Files.isSymbolicLink(f.toPath())
    }.getOrDefault(false)

    /**
     * Rated like the desktop: APK installers already installed and
     * thumbnails are safe (both regenerate or are copies); large files and
     * old downloads need review. App caches are listed from StorageStats so
     * the user sees where space went, but only Settings can clear another
     * app's cache on Android 11+, so they carry no path.
     */
    fun cleanup(allFiles: Boolean, usageAccess: Boolean): List<CleanupItem> {
        val out = ArrayList<CleanupItem>()
        if (allFiles) {
            val root = File(root)
            File(root, "DCIM/.thumbnails").takeIf { it.isDirectory }?.let { t ->
                val size = sizeOf(t)
                if (size > 0) out += CleanupItem("thumbnails", t.name, t.path, null, size, "safe")
            }
            val pm = context.packageManager
            val installed = pm.getInstalledPackages(0).map { it.packageName }.toHashSet()
            val download = File(root, Environment.DIRECTORY_DOWNLOADS)
            download.walkTopDown().maxDepth(3).filter { it.isFile }.forEach { f ->
                if (f.extension.equals("apk", true)) {
                    val pkg = runCatching { pm.getPackageArchiveInfo(f.path, 0)?.packageName }.getOrNull()
                    val rating = if (pkg != null && pkg in installed) "safe" else "review"
                    out += CleanupItem("apk", f.name, f.path, pkg, f.length(), rating)
                }
            }
            root.walkTopDown()
                .onEnter { !it.name.startsWith(".") && it.name != "Android" }
                .maxDepth(6)
                .filter { it.isFile && it.length() >= LARGE_FILE }
                .forEach { out += CleanupItem("largeFile", it.name, it.path, null, it.length(), "review") }
            root.walkTopDown()
                .onEnter { it.name != "Android" }
                .maxDepth(4)
                .filter { it.isDirectory && it != root && it.listFiles()?.isEmpty() == true }
                .take(200)
                .forEach { out += CleanupItem("emptyFolder", it.name, it.path, null, 0, "safe") }
        }
        if (usageAccess) {
            val pm = context.packageManager
            pm.getInstalledApplications(0).forEach { app ->
                val cache = runCatching {
                    stats.queryStatsForPackage(StorageManager.UUID_DEFAULT, app.packageName, Process.myUserHandle()).cacheBytes
                }.getOrNull() ?: return@forEach
                if (cache >= APP_CACHE_MIN) {
                    out += CleanupItem("appCache", pm.getApplicationLabel(app).toString(), null, app.packageName, cache, "safe")
                }
            }
        }
        return out.sortedByDescending { it.bytes }
    }

    /** Refuses anything outside shared storage, so a stale item can never point the delete at app data. */
    fun delete(item: CleanupItem): Long? {
        val path = item.path ?: return null
        val file = File(path).canonicalFile
        if (!file.path.startsWith(File(root).canonicalPath + File.separator)) return null
        if (!file.exists()) return 0
        val size = sizeOf(file)
        val ok = if (file.isDirectory) file.deleteRecursively() else file.delete()
        return if (ok) size else null
    }

    private fun sizeOf(f: File): Long = if (f.isFile) f.length() else f.walkTopDown().filter { it.isFile }.sumOf { it.length() }

    private companion object {
        const val LARGE_FILE = 100L * 1024 * 1024
        const val APP_CACHE_MIN = 20L * 1024 * 1024
    }
}
