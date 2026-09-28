package app.vitals.wear.data

import android.content.Context
import app.vitals.core.WireJson
import app.vitals.core.wear.PcState
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer
import java.io.File

/**
 * The last [PcState] per PC, on disk, so a tile or complication rendered
 * straight after a reboot shows the last reading (with its age) rather than
 * nothing. No tokens live here; those stay in the encrypted pairing store.
 */
class StateCache(context: Context) {
    private val file = File(context.noBackupFilesDir, "pc-states.json")
    private val serializer = MapSerializer(String.serializer(), PcState.serializer())

    fun read(): Map<String, PcState> {
        if (!file.exists()) return emptyMap()
        return runCatching { WireJson.Lenient.decodeFromString(serializer, file.readText()) }
            // A file from an older model is only a cache; the next update rewrites it.
            .getOrElse { emptyMap() }
    }

    fun write(states: Map<String, PcState>) {
        val tmp = File(file.parentFile, "${file.name}.tmp")
        tmp.writeText(WireJson.Lenient.encodeToString(serializer, states))
        if (!tmp.renameTo(file)) {
            file.delete()
            tmp.renameTo(file)
        }
    }
}
