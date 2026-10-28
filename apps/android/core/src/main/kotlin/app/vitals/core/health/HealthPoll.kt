package app.vitals.core.health

/**
 * What the Health page does with one answer from vmui, and how long it waits
 * before asking again. Pure, so the policy is tested without a screen.
 *
 * Once a minute while the page is visible (the caller stops polling when it
 * is hidden), as the contract asks. When vmui does not answer the wait
 * doubles up to five minutes, so a phone left on the page while the
 * server is down does not keep knocking.
 */
sealed interface HealthPoll {
    val delayMs: Long

    /** Show this answer; it replaces whatever was on screen. */
    data class Show(val health: MeHealth, override val delayMs: Long) : HealthPoll

    /** vmui did not answer: keep the last answer on screen, marked stale. */
    data class Stale(val failure: HealthFailure, override val delayMs: Long) : HealthPoll

    /**
    * Polling cannot help: the token was revoked ([HealthFailure.SignedOut])
    * or the account is not the owner's. The page says so and stops until
    * the user acts.
     */
    data class Stop(val failure: HealthFailure) : HealthPoll {
        override val delayMs: Long get() = Long.MAX_VALUE
    }

    companion object {
        const val VISIBLE_MS = 60_000L
        const val MAX_BACKOFF_MS = 300_000L

        /**
         * [failures] is how many answers in a row were [HealthFailure.Unreachable]
         * before this one; the caller resets it on success.
         */
        fun after(result: HealthResult<MeHealth>, failures: Int): HealthPoll = when (result) {
            is HealthResult.Ok -> Show(result.value, VISIBLE_MS)
            is HealthResult.Err -> when (val f = result.failure) {
                is HealthFailure.Unreachable -> Stale(f, backoff(failures + 1))
                // A body this build cannot read will not change in a minute,
                // but a vmui fix might land later: ask again, slowly.
                HealthFailure.Invalid -> Stale(f, MAX_BACKOFF_MS)
                HealthFailure.SignedOut, HealthFailure.NotOwner -> Stop(f)
            }
        }

        /** 60 s after the first miss, then 120, 240, capped at [MAX_BACKOFF_MS]. */
        fun backoff(failures: Int): Long {
            if (failures <= 1) return VISIBLE_MS
            val shift = (failures - 1).coerceAtMost(8)
            return (VISIBLE_MS shl shift).coerceAtMost(MAX_BACKOFF_MS)
        }
    }
}
