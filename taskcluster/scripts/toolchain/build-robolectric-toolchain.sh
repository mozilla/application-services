#!/bin/bash
#
# Build the Robolectric maven packages for branch builds.
#
# For unknown reasons, these sometimes fail to automatically build when running
# gradle, so we build them upfront and distribute the maven packages.

set -ex

mvn dependency:get -Dartifact=org.robolectric:android-all::6.0.1_r3-robolectric-r1
mvn dependency:get -Dartifact=org.robolectric:android-all:7.0.0_r1-robolectric-r1
mvn dependency:get -Dartifact=org.robolectric:android-all:8.0.0_r4-robolectric-r1
mvn dependency:get -Dartifact=org.robolectric:android-all:8.1.0-robolectric-4611349
mvn dependency:get -Dartifact=org.robolectric:android-all:9-robolectric-4913185

# Tar everything into UPLOAD_DIR
cd "$HOME"
mkdir -p "$UPLOAD_DIR"
tar zc --directory=/builds/worker/ --file="$UPLOAD_DIR"/robolectric.tar.gz .m2/repository/org/robolectric/
