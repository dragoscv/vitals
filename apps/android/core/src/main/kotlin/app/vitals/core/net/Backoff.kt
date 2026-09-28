package app.vitals.core.net

import kotlin.random.Random

/**
 * Reconnect delays: 500 ms doubling to 30 s, with ±20 % jitter so several
 * phones woken by the same Wi-Fi reconnect do not stampede the PC together.
 * Same curve as `packages/client/src/backoff.ts`.
 */
class Backoff(
    private val initialMs: Long = 500,
    private val maxMs: Long = 30_000,
    private val random: Random = Random.Default,
) {
    private var attempt = 0

    fun next(): Long {
        val base = (initialMs shl attempt.coerceAtMost(16)).coerceAtMost(maxMs)
        attempt++
        val jitter = base * 0.2
        return (base + random.nextDouble(-jitter, jitter)).toLong().coerceAtLeast(0)
    }

    fun reset() {
        attempt = 0
    }

    val failures: Int get() = attempt
}
