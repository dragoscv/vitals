package app.vitals.phone.ui

import androidx.compose.animation.ContentTransform
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.material3.SnackbarHostState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.IntOffset
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.navigation3.rememberViewModelStoreNavEntryDecorator
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.runtime.rememberSaveableStateHolderNavEntryDecorator
import androidx.navigation3.ui.NavDisplay
import app.vitals.phone.data.GlassMode
import app.vitals.phone.graph
import app.vitals.phone.ui.add.AddPcScreen
import app.vitals.phone.ui.detail.DetailScreen
import app.vitals.phone.ui.device.DeviceScreen
import app.vitals.phone.ui.machines.MachinesScreen
import app.vitals.phone.ui.nav.AddPc
import app.vitals.phone.ui.nav.Detail
import app.vitals.phone.ui.nav.Launch
import app.vitals.phone.ui.nav.Machines
import app.vitals.phone.ui.nav.Processes
import app.vitals.phone.ui.nav.Settings
import app.vitals.phone.ui.nav.ThisPhone
import app.vitals.phone.ui.processes.ProcessesScreen
import app.vitals.phone.ui.settings.SettingsScreen
import app.vitals.phone.ui.theme.DeviceTier
import app.vitals.phone.ui.theme.MeshBackground
import app.vitals.phone.ui.theme.ProvideGlass

/** Navigation for every screen: push, pop, and replace the stack from a shortcut. */
class Navigator(private val stack: MutableList<NavKey>) {
    fun push(key: NavKey) {
        stack.add(key)
    }

    fun pop() {
        if (stack.size > 1) stack.removeAt(stack.lastIndex)
    }

    fun home() {
        while (stack.size > 1) stack.removeAt(stack.lastIndex)
    }

    /** A bottom-bar tab: that tab's screen becomes the only one on the stack. */
    fun root(key: NavKey) {
        if (stack.size == 1 && stack[0] == key) return
        stack.add(key)
        while (stack.size > 1) stack.removeAt(0)
    }
}

val LocalNavigator = staticCompositionLocalOf<Navigator> { error("no navigator") }
val LocalSnackbar = staticCompositionLocalOf { SnackbarHostState() }

private val spatial = spring<IntOffset>(dampingRatio = Spring.DampingRatioNoBouncy, stiffness = Spring.StiffnessMediumLow)

@Composable
fun VitalsRoot(launch: Launch, launchSerial: Int) {
    val context = LocalContext.current
    val graph = context.graph
    val glassMode by graph.settings.glass.collectAsStateWithLifecycle(GlassMode.Auto)
    val blur = remember(glassMode) {
        when (glassMode) {
            GlassMode.Auto -> DeviceTier.supportsBlur(context)
            GlassMode.On -> android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S
            GlassMode.Off -> false
        }
    }
    val stack = rememberNavBackStack(ThisPhone)
    val navigator = remember(stack) { Navigator(stack) }
    val snackbar = remember { SnackbarHostState() }
    val pairings by graph.pairings.pairings.collectAsStateWithLifecycle()

    LaunchedEffect(launchSerial) {
        if (launchSerial == 0) return@LaunchedEffect
        when (launch) {
            Launch.Home -> navigator.root(ThisPhone)
            Launch.Scan -> {
                navigator.root(Machines)
                navigator.push(AddPc(startWithScan = true))
            }
            Launch.Processes -> {
                navigator.root(Machines)
                val first = graph.pairings.load().firstOrNull()
                navigator.push(if (first != null) Processes(first.id) else AddPc())
            }
        }
    }

    ProvideGlass(blur) {
        CompositionLocalProvider(LocalNavigator provides navigator, LocalSnackbar provides snackbar) {
            MeshBackground {
                NavDisplay(
                    backStack = stack,
                    onBack = { navigator.pop() },
                    entryDecorators = listOf(
                        rememberSaveableStateHolderNavEntryDecorator(),
                        rememberViewModelStoreNavEntryDecorator(),
                    ),
                    transitionSpec = { forward() },
                    popTransitionSpec = { backward() },
                    predictivePopTransitionSpec = { _ -> predictiveBack() },
                    entryProvider = entryProvider {
                        entry<ThisPhone> { DeviceScreen() }
                        entry<Machines> { MachinesScreen() }
                        entry<Detail> { key ->
                            pairings.firstOrNull { it.id == key.pairingId }?.let { DetailScreen(it) }
                        }
                        entry<Processes> { key ->
                            pairings.firstOrNull { it.id == key.pairingId }?.let { ProcessesScreen(it) }
                        }
                        entry<AddPc> { key -> AddPcScreen(startWithScan = key.startWithScan) }
                        entry<Settings> { SettingsScreen() }
                    },
                )
            }
        }
    }
}

private fun forward(): ContentTransform =
    (slideInHorizontally(spatial) { it / 3 } + fadeIn()) togetherWith
        (slideOutHorizontally(spatial) { -it / 6 } + fadeOut())

private fun backward(): ContentTransform =
    (slideInHorizontally(spatial) { -it / 6 } + fadeIn()) togetherWith
        (slideOutHorizontally(spatial) { it / 3 } + fadeOut())

/** Follows the finger during predictive back: the page shrinks slightly, as the system animation does. */
private fun predictiveBack(): ContentTransform =
    (slideInHorizontally(spatial) { -it / 6 } + fadeIn()) togetherWith
        (scaleOut(targetScale = 0.9f) + fadeOut())
