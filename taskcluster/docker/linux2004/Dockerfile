# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# This is intended to be a minimal Dockerfile only to build nimbus-fml,
# nimbus-cli, and the nimbus-experimenter megazord with an older version of
# glibc.

FROM ubuntu:20.04

VOLUME /builds/worker/checkouts
VOLUME /builds/worker/.cache

# Add worker user

RUN mkdir -p /builds && \
    useradd -d /builds/worker -s /bin/bash -m worker && \
    chown worker:worker /builds/worker && \
    mkdir /builds/worker/artifacts && \
    chown worker:worker /builds/worker/artifacts

WORKDIR /builds/worker/

# Set up the language variables to avoid problems (we run locale-gen later).
ENV LANG en_US.UTF-8
ENV LANGUAGE en_US:en
ENV LC_ALL en_US.UTF-8

# Do not use fancy output on taskcluster
ENV TERM dumb

# Used to detect in scripts whether we are running on taskcluster
ENV CI 1
ENV CI_TASKCLUSTER true

ENV \
    # Some APT packages like 'tzdata' wait for user input on install by default.
    # https://stackoverflow.com/questions/44331836/apt-get-install-tzdata-noninteractive
    DEBIAN_FRONTEND=noninteractive

RUN apt-get update -qq \
    && apt-get install -qy --no-install-recommends \
        python3 \
        python3-pip \
        ##########################
        # CI-specific dependencies
        ##########################
        git \
        curl \
        # Will set up the timezone to UTC (?).
        tzdata \
        # To install UTF-8 locales.
        locales \
        # Required to rsync the `libs` folder after fetch (see taskcluster/ci/android-build/kind.yml)
        rsync \
        # Required for creating a venv for glean_parser
        python3-venv \
        # Required to zip targets after build
        zip \
        # Required to build nimbus-cli and nimbus-fml
        build-essential \
        gcc-aarch64-linux-gnu \
        libc6-dev-arm64-cross \
        binutils-aarch64-linux-gnu \
        # Required to build nimbus-experimenter megazord
    && apt-get clean

RUN pip3 install --upgrade pip
RUN pip3 install \
    pytest \
    pyyaml \
    toml \
    taskcluster \
    yapf

# tooltool
RUN \
    curl -sfSL --retry 5 --retry-delay 10 \
         -o /usr/local/bin/tooltool.py \
         https://raw.githubusercontent.com/mozilla-releng/tooltool/master/client/tooltool.py && \
         chmod +x /usr/local/bin/tooltool.py

# %include-run-task

ENV SHELL=/bin/bash \
    HOME=/builds/worker \
    PATH=/builds/worker/.local/bin:$PATH

# run-task needs to run as root (after initialization, it changes to `worker`)
USER root
