package app.vitals.wear

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.wear.compose.material3.AppScaffold
import androidx.wear.compose.material3.TimeText
import androidx.wear.compose.navigation.SwipeDismissableNavHost
import androidx.wear.compose.navigation.composable
import androidx.wear.compose.navigation.rememberSwipeDismissableNavController
import app.vitals.core.pairing.Scope
import app.vitals.wear.ui.PcDetailScreen
import app.vitals.wear.ui.PcListScreen
import app.vitals.wear.ui.ProcessesScreen
import app.vitals.wear.ui.SensorsScreen
import app.vitals.wear.ui.VitalsTheme
import app.vitals.wear.ui.WearController
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val controller = WearController(VitalsWearApp.from(this).repository)

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) { controller.pollWhileVisible(this) }
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            // The platform call, not registerForActivityResult: Play services
            // brings an old Fragment onto the classpath that mis-routes that
            // result, and the answer is not needed here anyway — Alerter checks
            // the grant each time it posts.
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 1)
        }

        val device = VitalsWearApp.from(this).device
        setContent { VitalsTheme { VitalsNav(controller, device) } }
    }
}

private object Routes {
    const val LIST = "list"
    const val SELF = "self"
    const val DETAIL = "pc/{id}"
    const val SENSORS = "pc/{id}/sensors"
    const val PROCESSES = "pc/{id}/processes"

    fun detail(id: String) = "pc/${android.net.Uri.encode(id)}"
    fun sensors(id: String) = "${detail(id)}/sensors"
    fun processes(id: String) = "${detail(id)}/processes"
}

@Composable
private fun VitalsNav(controller: WearController, device: app.vitals.device.DeviceMonitor) {
    val nav = rememberSwipeDismissableNavController()
    val states by controller.states.collectAsStateWithLifecycle()
    val pairings by controller.pairings.collectAsStateWithLifecycle()
    val loaded by controller.loaded.collectAsStateWithLifecycle()

    AppScaffold(timeText = { TimeText() }) {
        SwipeDismissableNavHost(navController = nav, startDestination = Routes.LIST) {
            composable(Routes.LIST) {
                PcListScreen(
                    pairings, states, loaded,
                    onOpen = { nav.navigate(Routes.detail(it)) },
                    device = device,
                    onOpenSelf = { nav.navigate(Routes.SELF) },
                )
            }
            composable(Routes.SELF) { app.vitals.wear.ui.WatchSelfScreen(device) }
            composable(Routes.DETAIL) { entry ->
                val id = remember(entry) { entry.arguments?.getString("id").orEmpty() }
                PcDetailScreen(
                    state = states[id],
                    label = pairings.firstOrNull { it.id == id }?.label ?: states[id]?.label.orEmpty(),
                    onSensors = { nav.navigate(Routes.sensors(id)) },
                    onProcesses = { nav.navigate(Routes.processes(id)) },
                )
            }
            composable(Routes.SENSORS) { entry ->
                val id = remember(entry) { entry.arguments?.getString("id").orEmpty() }
                val state = states[id]
                SensorsScreen(sensors = state?.sensors.orEmpty(), reachable = state?.summary != null)
            }
            composable(Routes.PROCESSES) { entry ->
                val id = remember(entry) { entry.arguments?.getString("id").orEmpty() }
                val state = states[id]
                val pairing = pairings.firstOrNull { it.id == id }
                ProcessesScreen(
                    processes = state?.summary?.top.orEmpty(),
                    // Both must agree: the phone's view of the token and the watch's own.
                    canControl = (state?.canControl ?: false) && pairing != null && pairing.scope != Scope.Read,
                    onEndTask = { p -> controller.endTask(id, p.key) },
                )
            }
        }
    }
}
