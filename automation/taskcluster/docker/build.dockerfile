# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# We use this specific version because our decision task also runs on this one.
# We also use that same version in decisionlib.py
FROM ubuntu:bionic-20180821

MAINTAINER Nick Alexander "nalexander@mozilla.com"

# Configuration

ENV ANDROID_BUILD_TOOLS "28.0.3"
ENV ANDROID_SDK_VERSION "3859397"
ENV ANDROID_PLATFORM_VERSION "28"

ENV LANG en_US.UTF-8

# Do not use fancy output on taskcluster
ENV TERM dumb

ENV GRADLE_OPTS -Xmx4096m -Dorg.gradle.daemon=false

# Used to detect in scripts whether we are running on taskcluster
ENV CI_TASKCLUSTER true

ENV \
    #
    # Some APT packages like 'tzdata' wait for user input on install by default.
    # https://stackoverflow.com/questions/44331836/apt-get-install-tzdata-noninteractive
    DEBIAN_FRONTEND=noninteractive

# System.

RUN apt-get update -qq \
    # We need to install tzdata before all of the other packages. Otherwise it will show an interactive dialog that
    # we cannot navigate while building the Docker image.
    && apt-get install -qy tzdata \
    && apt-get install -qy --no-install-recommends openjdk-8-jdk \
                          wget \
                          expect \
                          git \
                          curl \
                          # For `cc` crates; see https://github.com/jwilm/alacritty/issues/1440.
                          g++ \
                          clang \
                          python \
                          python-pip \
                          python-setuptools \
                          locales \
                          unzip \
                          xz-utils \
                          make \
                          tclsh \
                          patch \
                          file \
                          # NSS build dependencies
                          gyp \
                          ninja-build \
                          zlib1g-dev \
                          # Delete mercurial once `libs/build-all.sh` gets NSS through a zip file.
                          mercurial \
                          # Delete p7zip once NSS windows is actually compiled instead of downloaded.
                          p7zip-full \
                          # End of NSS build dependencies
    && apt-get clean

RUN pip install --upgrade pip
RUN pip install 'taskcluster>=4,<5'
RUN pip install pyyaml

RUN locale-gen en_US.UTF-8

# Android SDK

RUN mkdir -p /build/android-sdk
WORKDIR /build

ENV ANDROID_HOME /build/android-sdk
ENV ANDROID_SDK_HOME /build/android-sdk
ENV PATH ${PATH}:${ANDROID_SDK_HOME}/tools:${ANDROID_SDK_HOME}/tools/bin:${ANDROID_SDK_HOME}/platform-tools:/opt/tools:${ANDROID_SDK_HOME}/build-tools/${ANDROID_BUILD_TOOLS}

RUN curl -L https://dl.google.com/android/repository/sdk-tools-linux-${ANDROID_SDK_VERSION}.zip > sdk.zip \
    && unzip -q sdk.zip -d ${ANDROID_SDK_HOME} \
    && rm sdk.zip \
    && mkdir -p /build/android-sdk/.android/ \
    && touch /build/android-sdk/.android/repositories.cfg \
    && yes | sdkmanager --licenses \
    && sdkmanager --verbose "platform-tools" \
        "platforms;android-${ANDROID_PLATFORM_VERSION}" \
        "build-tools;${ANDROID_BUILD_TOOLS}" \
        "extras;android;m2repository" \
        "extras;google;m2repository"

# Android NDK

# r15c agrees with mozilla-central and, critically, supports the --deprecated-headers flag needed to
# build OpenSSL
ENV ANDROID_NDK_VERSION "r15c"

# $ANDROID_NDK_ROOT is the preferred name, but the android gradle plugin uses $ANDROID_NDK_HOME.
ENV ANDROID_NDK_ROOT /build/android-ndk
ENV ANDROID_NDK_HOME /build/android-ndk

RUN curl -L https://dl.google.com/android/repository/android-ndk-${ANDROID_NDK_VERSION}-linux-x86_64.zip > ndk.zip \
	&& unzip -q ndk.zip -d /build \
	&& rm ndk.zip \
  && mv /build/android-ndk-${ANDROID_NDK_VERSION} ${ANDROID_NDK_ROOT}

ENV ANDROID_NDK_TOOLCHAIN_DIR /root/.android-ndk-r15c-toolchain
ENV ANDROID_NDK_API_VERSION 21

# Rust (cribbed from https://github.com/rust-lang-nursery/docker-rust/blob/ced83778ec6fea7f63091a484946f95eac0ee611/1.27.1/stretch/Dockerfile)

RUN set -eux; \
    rustArch='x86_64-unknown-linux-gnu'; rustupSha256='ce09d3de51432b34a8ff73c7aaa1edb64871b2541d2eb474441cedb8bf14c5fa'; \
    url="https://static.rust-lang.org/rustup/archive/1.17.0/${rustArch}/rustup-init"; \
    wget "$url"; \
    echo "${rustupSha256} *rustup-init" | sha256sum -c -; \
    chmod +x rustup-init; \
    ./rustup-init -y --no-modify-path --default-toolchain none; \
    rm rustup-init

ENV PATH=/root/.cargo/bin:$PATH

RUN \
    curl --silent --show-error --fail --location --retry 5 --retry-delay 10 \
        https://github.com/mozilla/sccache/releases/download/0.2.8/sccache-0.2.8-x86_64-unknown-linux-musl.tar.gz \
        | tar -xz --strip-components=1 -C /usr/local/bin/ \
            sccache-0.2.8-x86_64-unknown-linux-musl/sccache

RUN \
    curl --location --retry 10 --retry-delay 10 \
         -o /usr/local/bin/tooltool.py \
         https://raw.githubusercontent.com/mozilla/build-tooltool/36511dae0ead6848017e2d569b1f6f1b36984d40/tooltool.py && \
         chmod +x /usr/local/bin/tooltool.py

RUN git init repo
