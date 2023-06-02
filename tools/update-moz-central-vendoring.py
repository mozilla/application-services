#!/usr/bin/python

import argparse
import pathlib
import re
import subprocess

APP_SERVICES_DEPENDENCY_RE = re.compile(
    r'([\w-]+).*{\s*git\s*=\s*"https://github.com/mozilla/application-services"'
)
APP_SERVICES_ROOT = pathlib.Path(__file__).parent.parent

def main():
    args = parse_args()
    moz_central_root = pathlib.Path(args.moz_central_dir)
    app_services_rev = get_app_services_rev()
    update_cargo_toml(moz_central_root / "Cargo.toml", app_services_rev)
    subprocess.run(["./mach", "vendor", "rust"], cwd=moz_central_root)

    print("The vendoring was successful")
    print(" - If you saw a warning saying `There are 2 different versions of crate X`, then "
          "follow the listed steps to resolve that issue")
    print(" - Run `./mach cargo vet` to manually vet any new dependencies")
    print(" - Commit any changes and submit a phabricator patch")

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
    with open(cargo_toml_path, "r") as f:
        lines = f.readlines()
        for i in range(len(lines)):
            line = lines[i]
            m = APP_SERVICES_DEPENDENCY_RE.match(line)
            if m:
                crate = m.group(1)
                lines[i] = f'{crate} = {{ git = "https://github.com/mozilla/application-services", rev = "{app_services_rev}" }}\n'

    with open(cargo_toml_path, "w") as f:
        f.write("".join(lines))

if __name__ == "__main__":
    main()
