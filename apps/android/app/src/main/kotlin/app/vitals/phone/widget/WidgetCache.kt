package app.vitals.phone.widget

import android.content.Context
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import androidx.glance.appwidget.updateAll
import app.vitals.core.WireJson
import app.vitals.core.model.Summary
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.MapSerializer
import kotlinx.serialization.builtins.serializer

private val Context.widgetStore by preferencesDataStore(name = "widgets")

/** What a widget or the tile last learnt about one PC. [summary] is null when it could not be reached. */
@Serializable
data class CachedPc(
    val pairingId: String,
    val label: String,
    val fetchedMs: Long,
    val summary: Summary?,
)

/**
 * The last summary of each PC, for surfaces that render without a network
 * call of their own: Glance draws from this, and WorkManager fills it.
 */
class WidgetCache(context: Context) {
    private val app = context.applicationContext
    private val store = app.widgetStore
    private val serializer = MapSerializer(String.serializer(), CachedPc.serializer())

    val all: Flow<Map<String, CachedPc>> = store.data.map { prefs ->
        prefs[KEY]?.let { runCatching { WireJson.Lenient.decodeFromString(serializer, it) }.getOrNull() }.orEmpty()
    }

    suspend fun put(pairingId: String, label: String, summary: Summary?) {
        store.edit { prefs ->
            val current = prefs[KEY]
                ?.let { runCatching { WireJson.Lenient.decodeFromString(serializer, it) }.getOrNull() }
                .orEmpty()
            val previous = current[pairingId]
            // An unreachable PC keeps its last good numbers, marked by age,
            // rather than blanking a widget the user glances at.
            val entry = if (summary == null && previous != null) {
                previous.copy(label = label)
            } else {
                CachedPc(pairingId, label, System.currentTimeMillis(), summary)
            }
            prefs[KEY] = WireJson.Lenient.encodeToString(serializer, current + (pairingId to entry))
        }
        refreshSurfaces()
    }

    suspend fun retain(ids: Set<String>) {
        store.edit { prefs ->
            val current = prefs[KEY]
                ?.let { runCatching { WireJson.Lenient.decodeFromString(serializer, it) }.getOrNull() }
                .orEmpty()
            prefs[KEY] = WireJson.Lenient.encodeToString(serializer, current.filterKeys { it in ids })
        }
        refreshSurfaces()
    }

    private suspend fun refreshSurfaces() {
        runCatching {
            SmallWidget().updateAll(app)
            WideWidget().updateAll(app)
        }
    }

    private companion object {
        val KEY = stringPreferencesKey("pcs")
    }
}
