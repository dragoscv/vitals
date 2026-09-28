# :core's consumer rules cover app.vitals.core.model only. The watch also
# decodes the Data Layer contract (PcState, WatchPairing, WatchControl...) and
# the pairing store; kotlinx.serialization's bundled rules keep generated
# serializers for classes reached through serializer(), but a companion
# stripped by R8 surfaces as a SerializationException only on a release build.
-keep,includedescriptorclasses class app.vitals.core.wear.**$$serializer { *; }
-keepclassmembers class app.vitals.core.wear.** {
    *** Companion;
}
-keep,includedescriptorclasses class app.vitals.core.pairing.**$$serializer { *; }
-keepclassmembers class app.vitals.core.pairing.** {
    *** Companion;
}
