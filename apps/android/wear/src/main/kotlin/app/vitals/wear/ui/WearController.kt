package app.vitals.wear.ui

import app.vitals.core.model.ProcessKey
import app.vitals.wear.data.ControlOutcome
import app.vitals.wear.data.WearRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive

/** The app screen's view of the repository: its polling loop and its actions. */
class WearController(private val repository: WearRepository) {
    val states = repository.states
    val pairings = repository.pairings
    val loaded = repository.loaded

    /**
     * Refreshes every five seconds for as long as the caller's scope lives.
     * The activity runs this under `repeatOnLifecycle(STARTED)`, so the
     * moment the screen goes off, the polling stops.
     */
    suspend fun pollWhileVisible(scope: CoroutineScope) {
        while (scope.isActive) {
            repository.refresh()
            delay(POLL_MS)
        }
    }

    suspend fun endTask(pairingId: String, key: ProcessKey): ControlOutcome = repository.endTask(pairingId, key)

    private companion object {
        const val POLL_MS = 5_000L
    }
}
