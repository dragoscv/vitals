package app.vitals.core

import kotlinx.serialization.json.Json

/**
 * How the apps read the wire.
 *
 * `ignoreUnknownKeys` is on in production so a newer desktop that adds a
 * field does not break an older phone — the `modelVersion` check is what
 * guards a real shape change. The contract test uses [Strict], which fails
 * on any unknown key, so the two sides cannot drift unnoticed.
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
}
