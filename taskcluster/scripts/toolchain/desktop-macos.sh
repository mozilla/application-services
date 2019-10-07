pushd libs
./build-all.sh darwin
popd
tar -czf /build/repo/target.tar.gz libs/desktop
