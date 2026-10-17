package app.vitals.phone

import android.app.Application
import android.content.Context
import app.vitals.core.net.LanNetwork
import app.vitals.phone.data.AppGraph

class VitalsApp : Application() {
    lateinit var graph: AppGraph
        private set

    override fun onCreate() {
        super.onCreate()
        // Before the graph: every authenticated request consults it (AND-6).
        LanNetwork.install(this)
        graph = AppGraph(this)
        graph.start()
    }
}

/** One graph per process: a single OkHttp pool, one socket per PC however many screens watch it. */
val Context.graph: AppGraph get() = (applicationContext as VitalsApp).graph
