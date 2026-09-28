package app.vitals.phone.ui.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.TopAppBarScrollBehavior
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import app.vitals.phone.R
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.nav.Machines
import app.vitals.phone.ui.nav.Settings
import app.vitals.phone.ui.nav.ThisPhone
import app.vitals.phone.ui.theme.glassSource
import app.vitals.phone.ui.theme.glassSurface

enum class Tab { Phone, Machines, Settings }

/**
 * Every screen's frame. Content scrolls under a glass top bar (and, on the
 * two top-level screens, a glass navigation bar): the bars are transparent
 * containers over [glassSurface], and the content marks itself as the
 * [glassSource] they frost.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GlassScaffold(
    title: String,
    modifier: Modifier = Modifier,
    tab: Tab? = null,
    showBack: Boolean = tab == null,
    actions: @Composable RowScope.() -> Unit = {},
    floatingActionButton: @Composable () -> Unit = {},
    content: @Composable (PaddingValues) -> Unit,
) {
    val navigator = LocalNavigator.current
    val scroll: TopAppBarScrollBehavior = TopAppBarDefaults.pinnedScrollBehavior()
    val clear = TopAppBarDefaults.topAppBarColors(
        containerColor = Color.Transparent,
        scrolledContainerColor = Color.Transparent,
    )
    // A transparent container has no matching "on" colour, so Material falls
    // back to black for everything inside it: in dark mode every word on
    // every screen measured 1.2:1 against the page (A51, 2026-09-28). The
    // content colour is stated rather than derived for that reason.
    val onSurface = MaterialTheme.colorScheme.onSurface
    Scaffold(
        modifier = modifier.nestedScroll(scroll.nestedScrollConnection),
        containerColor = Color.Transparent,
        contentColor = onSurface,
        snackbarHost = { SnackbarHost(LocalSnackbar.current) },
        floatingActionButton = floatingActionButton,
        topBar = {
            CenterAlignedTopAppBar(
                modifier = Modifier.glassSurface(),
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    if (showBack) {
                        IconButton(onClick = { navigator.pop() }) {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back))
                        }
                    }
                },
                actions = actions,
                colors = clear,
                scrollBehavior = scroll,
            )
        },
        bottomBar = {
            if (tab != null) {
                NavigationBar(modifier = Modifier.glassSurface(), containerColor = Color.Transparent, contentColor = onSurface) {
                    NavigationBarItem(
                        selected = tab == Tab.Phone,
                        onClick = { navigator.root(ThisPhone) },
                        icon = { Icon(Icons.Filled.Home, null) },
                        label = { Text(stringResource(R.string.nav_phone)) },
                    )
                    NavigationBarItem(
                        selected = tab == Tab.Machines,
                        onClick = { navigator.root(Machines) },
                        icon = { Icon(Icons.Filled.Build, null) },
                        label = { Text(stringResource(R.string.nav_machines)) },
                    )
                    NavigationBarItem(
                        selected = tab == Tab.Settings,
                        onClick = { navigator.root(Settings) },
                        icon = { Icon(Icons.Filled.Settings, null) },
                        label = { Text(stringResource(R.string.nav_settings)) },
                    )
                }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().glassSource()) { content(padding) }
    }
}
