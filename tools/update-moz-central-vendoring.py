#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import argparse
import pathlib
import re
import subprocess
import sys

APP_SERVICES_DEPENDENCY_RE = re.compile(
    r'([\w-]+).*{\s*git\s*=\s*"https://github.com/mozilla/application-services"'
)
APP_SERVICES_ROOT = pathlib.Path(__file__).parent.parent


def main():
    args = parse_args()
    moz_central_root = pathlib.Path(args.moz_central_dir)
    app_services_rev = get_app_services_rev()
    update_cargo_toml(moz_central_root / "Cargo.toml", app_services_rev)
    run_process(["./mach", "vendor", "rust"], cwd=moz_central_root)
    run_process(["./mach", "uniffi", "generate"], cwd=moz_central_root)

    print("The vendoring was successful")
    print(
        " - If you saw a warning saying `There are 2 different versions of crate X`, then "
        "follow the listed steps to resolve that issue"
    )
    print(" - Run `./mach cargo vet` to manually vet any new dependencies")
    print(" - Commit any changes and submit a phabricator patch")
    print()
    print(
        "Details here: https://github.com/mozilla/application-services/blob/main/docs/howtos/vendoring-into-mozilla-central.md"
    )


def run_process(command, cwd):
    result = subprocess.run(command, cwd=cwd, check=False)
    if result.returncode != 0:
        print("Vendoring failed, please see above errors", file=sys.stderr)
        sys.exit(1)


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("moz_central_dir")
    return parser.parse_args()


def get_app_services_rev():
    return subprocess.check_output(
        ["git", "rev-parse", "HEAD"],
        encoding="utf-8",
        cwd=APP_SERVICES_ROOT,
    ).strip()


def update_cargo_toml(cargo_toml_path, app_services_rev):
    print(f"Updating application-services revision to {app_services_rev}")
    with open(cargo_toml_path) as f:
        lines = f.readlines()
        for i in range(len(lines)):
            line = lines[i]
            m = APP_SERVICES_DEPENDENCY_RE.match(line)
            if m:
                crate = m.group(1)
                lines[i] = (
                    f'{crate} = {{ git = "https://github.com/mozilla/application-services", rev = "{app_services_rev}" }}\n'
                )

    with open(cargo_toml_path, "w") as f:
        f.write("".join(lines))


if __name__ == "__main__":
    main()
