package app.vitals.phone.ui.nav

import androidx.navigation3.runtime.NavKey
import kotlinx.serialization.Serializable

// Serializable so the back stack survives process death: Android can kill
// the app behind the camera or a permission dialog and restore it later.

@Serializable
data object ThisPhone : NavKey

@Serializable
data object Machines : NavKey

@Serializable
data class Detail(val pairingId: String) : NavKey

@Serializable
data class Processes(val pairingId: String) : NavKey

@Serializable
data class AddPc(val startWithScan: Boolean = false) : NavKey

@Serializable
data object Settings : NavKey

/** Where a launcher shortcut, the tile or a notification asks to land. */
enum class Launch { Home, Processes, Scan }
