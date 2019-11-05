#!/bin/sh

python3 ./tools/dependency_summary.py > ./DEPENDENCIES.md

python3 ./tools/dependency_summary.py --all-ios-targets --package megazord_ios > megazords/ios/DEPENDENCIES.md

python3 ./tools/dependency_summary.py --all-android-targets --package megazord > megazords/full/DEPENDENCIES.md
python3 ./tools/dependency_summary.py --all-android-targets --package megazord --format pom > megazords/full/android/dependency-licenses.xml

python3 ./tools/dependency_summary.py --all-android-targets --package fenix > megazords/fenix/DEPENDENCIES.md
python3 ./tools/dependency_summary.py --all-android-targets --package fenix --format pom > megazords/fenix/android/dependency-licenses.xml

python3 ./tools/dependency_summary.py --all-android-targets --package lockbox > megazords/lockbox/DEPENDENCIES.md
python3 ./tools/dependency_summary.py --all-android-targets --package lockbox --format pom > megazords/lockbox/android/dependency-licenses.xml
