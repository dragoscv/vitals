package app.vitals.core.model

import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/** An enum whose wire spelling is its own, so a serializer can map both ways. */
interface WireEnum {
    val wire: String
}

/**
 * Decodes a wire string to its enum value, and a string this build has never
 * heard of to [unknown].
 *
 * The desktop adds enum values between releases (a new alert kind, a new
 * process state). With the generated serializer one unknown value failed the
 * WHOLE frame, so a phone one version behind showed a PC as unreachable while
 * it was answering every request. `coerceInputValues` would not help: it only
 * applies to properties with a default, and the wire model has none on
 * purpose (see Wire.kt). Encoding writes the value's own spelling, so the
 * contract test's round trip still sees exactly what Rust wrote.
 */
abstract class TolerantEnumSerializer<E>(
    name: String,
    values: Array<E>,
    private val unknown: E,
) : KSerializer<E> where E : Enum<E>, E : WireEnum {
    private val byWire: Map<String, E> = values.associateBy { it.wire }

    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor("app.vitals.$name", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: E) = encoder.encodeString(value.wire)

    override fun deserialize(decoder: Decoder): E = byWire[decoder.decodeString()] ?: unknown
}
