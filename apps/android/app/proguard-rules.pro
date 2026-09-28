# :core ships the rules for the wire model (consumer-rules.pro). What is
# serialised only in this module is kept here: the Nav 3 routes (the back
# stack is saved through their serializers) and the widget cache.
-keep,includedescriptorclasses class app.vitals.phone.**$$serializer { *; }
-keepclassmembers class app.vitals.phone.** {
    *** Companion;
}
-keepclasseswithmembers class app.vitals.phone.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# The Wear contract lives in :core outside the model package, so the core
# consumer rules do not cover it; the watch decodes these by name.
-keep,includedescriptorclasses class app.vitals.core.wear.**$$serializer { *; }
-keep,includedescriptorclasses class app.vitals.core.pairing.**$$serializer { *; }
-keepclassmembers class app.vitals.core.wear.**, app.vitals.core.pairing.** {
    *** Companion;
}
