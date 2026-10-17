package app.vitals.core

import kotlinx.serialization.json.Json

/**
 * How the apps read the wire.
 *
 * `ignoreUnknownKeys` is on in production so a newer desktop that adds a
 * field does not break an older phone — the `modelVersion` check is what
 * guards a real shape change. The contract test uses [Strict], which fails
 * on any unknown key, so the two sides cannot drift unnoticed.
 *
 * Enums are tolerant on the wire (`model/TolerantEnum.kt`): a value a newer
 * desktop added decodes to the enum's `Unknown` variant instead of failing
 * the whole frame, and [decodeFailures] counts the frames that still could
 * not be read so a screen can say "update the app" rather than go blank.
 */
object WireJson {
    val Lenient: Json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        coerceInputValues = false
    }

    val Strict: Json = Json {
        ignoreUnknownKeys = false
        explicitNulls = true
    }

    private val failures = java.util.concurrent.atomic.AtomicLong()

    /** Frames and replies that failed to decode since the process started. */
    val decodeFailures: Long get() = failures.get()

    /** Records one failed decode; the caller then reports the PC as incompatible. */
    fun noteDecodeFailure() {
        failures.incrementAndGet()
    }
}
