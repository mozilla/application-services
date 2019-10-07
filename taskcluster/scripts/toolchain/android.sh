./libs/verify-android-environment.sh
pushd libs
./build-all.sh android
popd
tar -czf /build/repo/target.tar.gz libs/android
