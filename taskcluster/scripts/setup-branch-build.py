#!/usr/bin/python3

import argparse
import os
import subprocess

ANDROID_COMPONENTS_REPO_URL = 'https://github.com/bendk/android-components'
FENIX_REPO_URL = 'https://github.com/bendk/fenix'

def main():
    args = parse_args()
    local_properties = ["rust.targets=x86,linux-x86-64"]
    local_properties.extend(branch_build_properties('application-services', '.'))
    if args.android_components:
        git_checkout(ANDROID_COMPONENTS_REPO_URL, args.android_components)
        local_properties.extend(branch_build_properties('android-components', 'android-components'))
    if args.fenix:
        git_checkout(FENIX_REPO_URL, args.fenix)

    local_properties = '\n'.join(local_properties)
    print("Local properties:")
    print(local_properties)

    write_local_properties("local.properties", local_properties)
    if args.android_components:
        write_local_properties("android-components/local.properties", local_properties)
    if args.fenix:
        write_local_properties("fenix/local.properties", local_properties)

def parse_args():
    parser = argparse.ArgumentParser(description='Setup a branch build in taskcluster')
    parser.add_argument('--android-components', help='Android components branch')
    parser.add_argument('--fenix', help='Fenix branch')
    return parser.parse_args()

def git_checkout(url, branch):
    subprocess.check_call(['git', 'clone', '--branch', branch, '--recurse-submodules', '--depth', '1', '--', url])

def branch_build_properties(name, checkout_dir):
    checkout_dir = os.path.abspath(checkout_dir)
    commit_id = subprocess.check_output(['git', 'rev-parse', '--short', 'HEAD'], encoding='utf8', cwd=checkout_dir).strip()
    return [
        f'branchBuild.{name}.dir={checkout_dir}',
        f'branchBuild.{name}.version={commit_id}',
    ]

def write_local_properties(path, local_properties):
    path = os.path.abspath(path)
    print(f"Writing local properties to {path}")
    with open(path, 'w') as f:
        f.write(local_properties)

if __name__ == '__main__':
    main()
