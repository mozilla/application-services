# ProGuard rules for consumers of this library.

# JNA specific rules
# See https://github.com/java-native-access/jna/blob/master/www/FrequentlyAskedQuestions.md#jna-on-android
-dontwarn java.awt.*
-keep class com.sun.jna.* { *; }
-keep class * extends com.sun.jna.* { *; }
-keepclassmembers class * extends com.sun.jna.* { public *; }

####################################################################################################
# Add explicit keep rules for Nimbus RustBuffer and related structs to avoid
# overly-aggressive optimization when R8 fullMode is enabled, leading to crashes.
####################################################################################################
-keepattributes RuntimeVisibleAnnotations,RuntimeInvisibleAnnotations,RuntimeVisibleTypeAnnotations,RuntimeInvisibleTypeAnnotations,AnnotationDefault,InnerClasses,EnclosingMethod,Signature
-keep class org.mozilla.experiments.nimbus.internal.** { *; }
