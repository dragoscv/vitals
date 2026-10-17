# The device model (history rows, snapshots relayed to the UI) is read and
# written with kotlinx.serialization. Its bundled rules keep serializers
# reached through serializer(), but a companion R8 strips surfaces only on a
# release build as a SerializationException, so every app embedding :device
# keeps them explicitly, as :core does for the wire model.
-keep,includedescriptorclasses class app.vitals.device.**$$serializer { *; }
-keepclassmembers class app.vitals.device.** {
    *** Companion;
}
