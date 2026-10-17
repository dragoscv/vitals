# kotlinx.serialization keeps its generated serializers through its own
# bundled rules; the wire model only needs its companions kept.
-keep,includedescriptorclasses class app.vitals.core.model.**$$serializer { *; }
-keepclassmembers class app.vitals.core.model.** {
    *** Companion;
}
# Tolerant enums use hand-written serializer objects named in
# @Serializable(with = ...); keep them and their enum entries.
-keep class app.vitals.core.model.*$Serializer { *; }
-keepclassmembers enum app.vitals.core.model.** { *; }

# Pairings are stored and the pairing-code exchange is decoded by these
# serializers; every app that embeds :core needs them, not just the phone.
-keep,includedescriptorclasses class app.vitals.core.pairing.**$$serializer { *; }
-keep,includedescriptorclasses class app.vitals.core.net.**$$serializer { *; }
-keepclassmembers class app.vitals.core.pairing.**, app.vitals.core.net.** {
    *** Companion;
}
