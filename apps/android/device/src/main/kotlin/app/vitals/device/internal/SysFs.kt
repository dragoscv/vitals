package app.vitals.device.internal

import java.io.File

/**
 * Reads a kernel file, or null when the platform refuses. SELinux denials,
 * missing nodes and busy files all look the same to an app and all mean the
 * same thing here: that reading is not available.
 */
internal object SysFs {
    fun read(path: String): String? = try {
        File(path).inputStream().bufferedReader().use { it.readText() }
    } catch (_: Exception) {
        null
    }

    fun readLong(path: String): Long? = read(path)?.trim()?.toLongOrNull()

    fun list(path: String): List<File> = try {
        File(path).listFiles()?.toList().orEmpty()
    } catch (_: SecurityException) {
        emptyList()
    }
}
