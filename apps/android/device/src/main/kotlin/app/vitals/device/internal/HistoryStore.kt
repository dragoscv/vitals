package app.vitals.device.internal

import app.vitals.device.model.DeviceSample
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.Json
import java.io.File

/**
 * Append-only JSON lines in the app's private files. A week at one sample a
 * minute is about 10 000 lines, roughly 2 MB: small enough that a database
 * would be more code than data, and a torn last line after a crash is simply
 * skipped on read.
 */
internal class HistoryStore(dir: File) {
    private val file = File(dir, "device-history.jsonl")
    private val lock = Mutex()
    private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

    suspend fun append(sample: DeviceSample) = lock.withLock {
        file.appendText(json.encodeToString(DeviceSample.serializer(), sample) + "\n")
    }

    suspend fun since(ms: Long): List<DeviceSample> = lock.withLock { readAll().filter { it.ts >= ms } }

    suspend fun prune(keepMs: Long) = lock.withLock {
        val cutoff = System.currentTimeMillis() - keepMs
        val all = readAll()
        val kept = all.filter { it.ts >= cutoff }
        if (kept.size == all.size) return@withLock
        val tmp = File(file.parentFile, file.name + ".tmp")
        tmp.writeText(kept.joinToString("") { json.encodeToString(DeviceSample.serializer(), it) + "\n" })
        tmp.renameTo(file)
    }

    private fun readAll(): List<DeviceSample> {
        if (!file.exists()) return emptyList()
        return file.readLines().mapNotNull { line ->
            if (line.isBlank()) null else runCatching { json.decodeFromString(DeviceSample.serializer(), line) }.getOrNull()
        }
    }
}
