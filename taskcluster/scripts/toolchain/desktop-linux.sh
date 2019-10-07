./libs/verify-android-environment.sh
pushd libs
./build-all.sh desktop
popd
tar -czf /build/repo/target.tar.gz libs/desktop
