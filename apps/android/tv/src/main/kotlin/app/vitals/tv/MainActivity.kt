package app.vitals.tv

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusDirection
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Settings
import androidx.tv.material3.DrawerValue
import androidx.tv.material3.Icon
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.NavigationDrawer
import androidx.tv.material3.NavigationDrawerItem
import androidx.tv.material3.Text
import androidx.tv.material3.rememberDrawerState
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.VitalsTvTheme
import app.vitals.tv.ui.add.AddPcScreen
import app.vitals.tv.ui.overview.OverviewScreen
import app.vitals.tv.ui.pc.PcListScreen
import app.vitals.tv.ui.pc.PcScreen
import app.vitals.tv.ui.settings.SettingsScreen
import app.vitals.tv.ui.device.TvDeviceScreen

/** Where the user is. A TV app has one window and one back button; a list is the whole navigation model. */
sealed interface Route {
    data object Overview : Route
    data object ThisTv : Route
    data object Pcs : Route
    data object Add : Route
    data object Settings : Route
    data class Pc(val id: String) : Route
}

class Navigator(private val stack: MutableList<Route>) {
    val current: Route get() = stack.last()

    fun push(r: Route) {
        stack.add(r)
    }

    /** A drawer destination replaces the stack: pressing Back from any top level leaves the app, as TV users expect. */
    fun top(r: Route) {
        stack.clear()
        stack.add(r)
    }

    fun pop(): Boolean = if (stack.size > 1) {
        stack.removeAt(stack.lastIndex)
        true
    } else {
        false
    }
}

val LocalNavigator = staticCompositionLocalOf<Navigator> { error("no navigator") }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { VitalsTvTheme { TvRoot() } }
    }
}

private data class Destination(val route: Route, val label: Int, val icon: @Composable () -> Painter)

private val Destinations = listOf(
    Destination(Route.Overview, R.string.nav_overview) { rememberVectorPainter(Icons.Filled.Home) },
    Destination(Route.ThisTv, R.string.nav_tv) { painterResource(R.drawable.ic_tv) },
    Destination(Route.Pcs, R.string.nav_pcs) { painterResource(R.drawable.ic_pc) },
    Destination(Route.Add, R.string.nav_add) { rememberVectorPainter(Icons.Filled.Add) },
    Destination(Route.Settings, R.string.nav_settings) { rememberVectorPainter(Icons.Filled.Settings) },
)

@Composable
private fun TvRoot() {
    val stack = remember { mutableStateListOf<Route>(Route.Overview) }
    val navigator = remember { Navigator(stack) }
    val drawer = rememberDrawerState(DrawerValue.Closed)
    val current = stack.last()
    val pairings by LocalContext.current.tvGraph.pairings.pairings.collectAsStateWithLifecycle()
    val focus = LocalFocusManager.current
    var handoff by remember { mutableIntStateOf(0) }
    // Read from the drawer's own scope: moving Right from inside a screen
    // would step to a neighbour instead of entering the screen.
    val drawerFocused = remember { androidx.compose.runtime.mutableStateOf(false) }
    val content = remember { androidx.compose.ui.focus.FocusRequester() }
    // A new screen means "go there": hand focus to it, as every Google TV app
    // does. Both ways in need it: OK on a drawer item leaves focus in the
    // drawer, and opening a PC removes the focused card, so focus fell back to
    // the drawer too (found on the Chromecast, 2026-09-29). Done two frames
    // later, because the new screen has no focusable yet when it is chosen.
    LaunchedEffect(handoff, stack.size, current) {
        if (handoff == 0 && stack.size == 1 && current == Route.Overview) return@LaunchedEffect
        withFrameNanos { }
        withFrameNanos { }
        // moveFocus(Right) from the drawer found nothing on the Chromecast;
        // asking the content's focus group hands focus to its first child.
        if (drawerFocused.value || handoff > 0) runCatching { content.requestFocus() }.onFailure { focus.moveFocus(FocusDirection.Right) }
    }

    BackHandler(enabled = stack.size > 1) { navigator.pop() }

    CompositionLocalProvider(LocalNavigator provides navigator) {
        Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
            // A standard (not modal) drawer: collapsed to icons it stays on
            // screen, so D-pad left from any content reaches it, and it never
            // covers the readings.
            NavigationDrawer(
                drawerState = drawer,
                drawerContent = { _ ->
                    drawerFocused.value = hasFocus
                    Column(Modifier.fillMaxHeight().padding(12.dp)) {
                        Destinations.forEach { d ->
                            val selected = when (current) {
                                is Route.Pc -> d.route == Route.Pcs
                                else -> current == d.route
                            }
                            NavigationDrawerItem(
                                selected = selected,
                                onClick = {
                                    navigator.top(d.route)
                                    handoff++
                                },
                                leadingContent = { Icon(d.icon(), contentDescription = null) },
                            ) { Text(stringResource(d.label)) }
                        }
                    }
                },
            ) {
                Box(Modifier.fillMaxSize().focusRequester(content).focusGroup()) {
                    when (val r = current) {
                        Route.Overview -> OverviewScreen()
                        Route.ThisTv -> TvDeviceScreen()
                        Route.Pcs -> PcListScreen()
                        Route.Add -> AddPcScreen()
                        Route.Settings -> SettingsScreen()
                        is Route.Pc -> {
                            val p = pairings.firstOrNull { it.id == r.id }
                            if (p != null) PcScreen(p) else PcListScreen()
                        }
                    }
                }
            }
        }
    }
}
