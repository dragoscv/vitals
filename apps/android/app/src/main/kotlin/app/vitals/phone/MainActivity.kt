package app.vitals.phone

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.lifecycle.lifecycleScope
import app.vitals.phone.service.WatchService
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import app.vitals.phone.ui.VitalsRoot
import app.vitals.phone.ui.nav.Launch
import app.vitals.phone.ui.theme.VitalsTheme
import app.vitals.phone.widget.WidgetWork

class MainActivity : ComponentActivity() {
    private var launch by mutableStateOf(Launch.Home)
    private var launchSerial by mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        // Held only until the encrypted pairing file is read, so the first
        // frame is the real home screen and not a flash of "no PCs yet".
        splash.setKeepOnScreenCondition { !graph.ready.value }
        if (savedInstanceState == null) take(intent)
        setContent {
            VitalsTheme {
                VitalsRoot(launch = launch, launchSerial = launchSerial)
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        take(intent)
    }

    override fun onStart() {
        super.onStart()
        // Opening the app is the cue to bring widgets and the tile up to date.
        WidgetWork.refreshNow(this)
        // If Android killed the process, a watch the user turned on would
        // otherwise stay off without saying so. Started from here because an
        // activity in front is allowed to start a foreground service.
        lifecycleScope.launch {
            if (graph.settings.watched.first().isNotEmpty()) WatchService.start(this@MainActivity)
        }
    }

    private fun take(intent: Intent?) {
        launch = when (intent?.action) {
            ACTION_PROCESSES -> Launch.Processes
            ACTION_SCAN -> Launch.Scan
            else -> return
        }
        launchSerial++
    }

    companion object {
        const val ACTION_PROCESSES = "app.vitals.action.PROCESSES"
        const val ACTION_SCAN = "app.vitals.action.SCAN"
    }
}
