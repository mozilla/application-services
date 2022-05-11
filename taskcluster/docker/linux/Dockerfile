# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

FROM ubuntu:18.04

MAINTAINER Edouard Oger "eoger@mozilla.com"

# Add worker user

RUN mkdir /builds && \
    useradd -d /builds/worker -s /bin/bash -m worker && \
    chown worker:worker /builds/worker && \
    mkdir /builds/worker/artifacts && \
    chown worker:worker /builds/worker/artifacts

WORKDIR /builds/worker/

# Configuration

ENV ANDROID_BUILD_TOOLS "30.0.3"
ENV ANDROID_PLATFORM_VERSION "30"
ENV ANDROID_NDK_VERSION "21.3.6528147"

# Set up the language variables to avoid problems (we run locale-gen later).
ENV LANG en_US.UTF-8
ENV LANGUAGE en_US:en
ENV LC_ALL en_US.UTF-8

# Do not use fancy output on taskcluster
ENV TERM dumb

ENV GRADLE_OPTS -Xmx4096m -Dorg.gradle.daemon=false

# Used to detect in scripts whether we are running on taskcluster
ENV CI 1
ENV CI_TASKCLUSTER true

ENV \
    # Some APT packages like 'tzdata' wait for user input on install by default.
    # https://stackoverflow.com/questions/44331836/apt-get-install-tzdata-noninteractive
    DEBIAN_FRONTEND=noninteractive

# System.

RUN apt-get update -qq \
    && apt-get install -qy --no-install-recommends \
        ####################
        # Build dependencies
        # If you add anything below, please update building.md.
        ####################
        # Android builds
        openjdk-11-jdk \
        # Required by gyp but also CI scripts.
        python \
        python-pip \
        python3 \
        python3-pip \
        # libs/ source patching.
        patch \
        # NSS build system.
        gyp ninja-build \
        # NSS dependency.
        zlib1g-dev \
        # SQLCipher build system.
        make \
        # SQLCipher dependency.
        tclsh \
        ##########################
        # CI-specific dependencies
        ##########################
        git \
        curl \
        # Required by symbolstore.py.
        file \
        # Will set up the timezone to UTC (?).
        tzdata \
        # To install UTF-8 locales.
        locales \
        # For `cc` crates; see https://github.com/jwilm/alacritty/issues/1440.
        # <TODO: Is this still true?>.
        g++ \
        # taskcluster > mohawk > setuptools.
        # Required to extract the Android SDK/NDK.
        unzip \
        # Required by tooltool to extract tar.xz archives.
        xz-utils \
        # For windows cross-compilation.
        mingw-w64 \
        # <TODO: Delete p7zip once NSS windows is actually compiled instead of downloaded>.
        p7zip-full \
        # Required to rsync the `libs` folder after fetch (see taskcluster/ci/android-build/kind.yml)
        rsync \
    && apt-get clean

RUN pip3 install --upgrade pip
RUN pip3 install \
    pyyaml \
    taskcluster \
    # Required to manipulate archiving operations for upcoming monitoring work
    zstandard

# Compile the UTF-8 english locale files (required by Python).
RUN locale-gen en_US.UTF-8

# Android SDK

RUN mkdir -p /builds/worker/android-sdk
WORKDIR /builds/worker

ENV ANDROID_HOME /builds/worker/android-sdk
ENV ANDROID_SDK_HOME /builds/worker/android-sdk
ENV PATH ${PATH}:${ANDROID_SDK_HOME}/cmdline-tools/latest/bin:${ANDROID_SDK_HOME}/platform-tools:/opt/tools:${ANDROID_SDK_HOME}/build-tools/${ANDROID_BUILD_TOOLS}

# Download the Android SDK tools, unzip them to ${ANDROID_SDK_HOME}/cmdline-tools/latest/, accept all licenses
# The download link comes from https://developer.android.com/studio/#downloads
RUN curl -sfSL --retry 5 --retry-delay 10 https://dl.google.com/android/repository/commandlinetools-linux-7583922_latest.zip > sdk.zip \
    && unzip -q sdk.zip \
    && mkdir $ANDROID_SDK_HOME/cmdline-tools \
    && mv cmdline-tools $ANDROID_HOME/cmdline-tools/latest \
    && rm sdk.zip \
    && mkdir -p /builds/worker/android-sdk/.android/ \
    && touch /builds/worker/android-sdk/.android/repositories.cfg \
    && yes | sdkmanager --licenses \
    && sdkmanager --verbose "platform-tools" \
        "platforms;android-${ANDROID_PLATFORM_VERSION}" \
        "build-tools;${ANDROID_BUILD_TOOLS}" \
        "extras;android;m2repository" \
        "extras;google;m2repository" \
        "ndk;21.3.6528147"

RUN chown -R worker:worker /builds/worker/android-sdk

# sccache
RUN \
    curl -sfSL --retry 5 --retry-delay 10 \
        https://github.com/mozilla/sccache/releases/download/0.2.11/sccache-0.2.11-x86_64-unknown-linux-musl.tar.gz \
        | tar -xz --strip-components=1 -C /usr/local/bin/ \
            sccache-0.2.11-x86_64-unknown-linux-musl/sccache

ENV CCACHE=sccache \
    RUSTC_WRAPPER=sccache \
    SCCACHE_IDLE_TIMEOUT=1200 \
    SCCACHE_CACHE_SIZE=40G \
    SCCACHE_ERROR_LOG=/builds/worker/sccache.log \
    RUST_LOG="sccache=info" \
    RUST_BACKTRACE=1 \
    RUSTFLAGS="-Dwarnings" \
    CARGO_INCREMENTAL=0

# tooltool
RUN \
    curl -sfSL --retry 5 --retry-delay 10 \
         -o /usr/local/bin/tooltool.py \
         https://raw.githubusercontent.com/mozilla/build-tooltool/36511dae0ead6848017e2d569b1f6f1b36984d40/tooltool.py && \
         chmod +x /usr/local/bin/tooltool.py

# %include-run-task

ENV SHELL=/bin/bash \
    HOME=/builds/worker \
    PATH=/builds/worker/.local/bin:/builds/worker/.cargo/bin:$PATH

VOLUME /builds/worker/checkouts
VOLUME /builds/worker/.cache

# Switch to the worker user to install rustup
# After startup, run-task will downgrade it's permissions to be this user
USER worker

# run-task needs to run as root (after initialization, it changes to `worker`)
USER root
