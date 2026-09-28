package app.vitals.wear

import android.app.Application
import android.content.Context
import app.vitals.wear.data.Alerter
import app.vitals.wear.data.WearRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

class VitalsWearApp : Application() {
    /**
     * Outlives any one tile or complication binding: those services are
     * unbound within milliseconds of answering, and a refresh they start
     * must still land so the next request has fresh data.
     */
    val appScope: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    val repository: WearRepository by lazy { WearRepository(this, appScope) }

    /** The watch itself (ADR-0035); samples only while a screen collects it. */
    val device: app.vitals.device.DeviceMonitor by lazy { app.vitals.device.AndroidDeviceMonitor(this) }

    override fun onCreate() {
        super.onCreate()
        Alerter.ensureChannel(this)
    }

    companion object {
        fun from(context: Context): VitalsWearApp = context.applicationContext as VitalsWearApp
    }
}
