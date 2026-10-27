# JNA reaches into its own classes by reflection, and the UniFFI bindings through JNA.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
# The certificate verifier is called into from Rust by name.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
-keep class io.github.deejaykyle.digida.Tls { *; }
-dontwarn java.awt.**
