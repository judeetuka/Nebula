# NEBULA ProGuard/R8 rules

# Keep Flutter engine
-keep class io.flutter.** { *; }

# Keep NEBULA platform bridge (called via JNI from Rust)
-keep class com.nebula.nebula_node.** { *; }
-keepclassmembers class com.nebula.nebula_node.platform.NebulaPlatformBridge {
    public static ** *(...);
}

# Obfuscate everything else
-repackageclasses ''
-allowaccessmodification
-optimizations !code/simplification/arithmetic
