#!/usr/bin/env bash

set -euvx

if [[ "${#}" -ne 1 ]]
then
    echo "Usage:"
    echo "./automation/upload_android_symbols.sh <symbols dir>"
    exit 1
fi

if [[ ! -f "$PWD/libs/android_defaults.sh" ]]
then
    echo "upload_android_symbols.sh must be executed from the root directory."
    exit 1
fi

SYMBOLS_DIR=${1}

if [ ! -d "${SYMBOLS_DIR}" ]; then
    echo "upload_android_symbols.sh: ${SYMBOLS_DIR} is not a directory"
    exit 1
fi

python3 -m venv upload-android-symbols-venv
upload-android-symbols-venv/bin/pip3 install -r automation/symbols-generation/requirements.txt
upload-android-symbols-venv/bin/python3 automation/symbols-generation/upload_symbols.py "${SYMBOLS_DIR}" -t "$PWD/.symbols_upload_token"
