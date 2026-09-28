package app.vitals.phone.data

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.settingsStore by preferencesDataStore(name = "settings")

enum class GlassMode { Auto, On, Off }

/** Preferences that are not secrets. Tokens stay in the Keystore-backed PairingStore. */
class SettingsStore(context: Context) {
    private val store = context.applicationContext.settingsStore

    val glass: Flow<GlassMode> = store.data.map { prefs ->
        prefs[GLASS]?.let { name -> GlassMode.entries.firstOrNull { it.name == name } } ?: GlassMode.Auto
    }

    /** Pairings the user asked to keep an eye on in the background. */
    val watched: Flow<Set<String>> = store.data.map { it[WATCHED].orEmpty() }

    /** The pairing the widgets and the tile show; the first one when unset. */
    val selected: Flow<String?> = store.data.map { it[SELECTED] }

    /**
     * Record this phone's history in the background (one sample every 15
     * minutes, WorkManager's floor) and raise its alerts as notifications.
     * On by default: it costs one two-second wake per quarter hour, and a
     * history that only exists after the user found the toggle is useless.
     */
    val deviceHistory: Flow<Boolean> = store.data.map { it[DEVICE_HISTORY] ?: true }

    suspend fun setDeviceHistory(on: Boolean) {
        store.edit { it[DEVICE_HISTORY] = on }
    }

    suspend fun setGlass(mode: GlassMode) {
        store.edit { it[GLASS] = mode.name }
    }

    suspend fun setWatched(id: String, on: Boolean) {
        store.edit {
            val current = it[WATCHED].orEmpty()
            it[WATCHED] = if (on) current + id else current - id
        }
    }

    suspend fun setSelected(id: String) {
        store.edit { it[SELECTED] = id }
    }

    suspend fun forget(id: String) {
        store.edit {
            it[WATCHED] = it[WATCHED].orEmpty() - id
            if (it[SELECTED] == id) it.remove(SELECTED)
        }
    }

    private companion object {
        val GLASS = stringPreferencesKey("glass")
        val WATCHED = stringSetPreferencesKey("watched")
        val SELECTED = stringPreferencesKey("selected")
        val DEVICE_HISTORY = booleanPreferencesKey("device_history")
    }
}
