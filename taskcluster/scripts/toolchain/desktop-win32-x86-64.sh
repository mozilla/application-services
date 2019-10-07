pushd libs
./build-all.sh win32-x86-64
popd
tar -czf /build/repo/target.tar.gz libs/desktop
