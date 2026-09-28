# kotlinx.serialization keeps its generated serializers through its own
# bundled rules; the wire model only needs its companions kept.
-keep,includedescriptorclasses class app.vitals.core.model.**$$serializer { *; }
-keepclassmembers class app.vitals.core.model.** {
    *** Companion;
}
