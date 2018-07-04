This directory requires Carol's sandvich libs.

From https://github.com/carolkng/application-services/tree/sand, copy
sandvich-android/app/src/main/jniLibs/*

You should end up with:

./arm64
./arm64/libcjose.so
./arm64/libcrypto.so
./arm64/libfxa_client.so
./arm64/libjansson.so
./arm64/libjnidispatch.so
./arm64/libssl.so
./armeabi
./armeabi/libcjose.so
./armeabi/libcrypto.so
./armeabi/libfxa_client.so
./armeabi/libjansson.so
./armeabi/libjnidispatch.so
./armeabi/libssl.so
./x86
./x86/libcjose.so
./x86/libcrypto.so
./x86/libfxa_client.so
./x86/libjansson.so
./x86/libjnidispatch.so
./x86/libssl.so

(although only the x86 folder probably matters for the emulator)