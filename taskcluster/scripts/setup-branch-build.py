#!/usr/bin/python3

import argparse
import os
import subprocess

def main():
    args = parse_args()
    local_properties = []
    local_properties.extend(branch_build_properties('application-services', '.'))
    if args.firefox_android_branch:
        git_checkout(firefox_android_repo(args), args.firefox_android_branch)
        local_properties.extend(branch_build_properties('android-components', 'firefox-android/android-components'))

    local_properties = '\n'.join(local_properties)
    print("Local properties:")
    print(local_properties)

    write_local_properties("local.properties", local_properties)
    if args.firefox_android_branch:
        write_local_properties("firefox-android/android-components/local.properties", local_properties)
        write_local_properties("firefox-android/fenix/local.properties", local_properties)

def parse_args():
    parser = argparse.ArgumentParser(description='Setup a branch build in taskcluster')
    parser.add_argument('--firefox-android-owner', help='firefox-android repository owner', default='mozilla-mobile')
    parser.add_argument('--firefox-android-branch', help='firefox-android branch')
    return parser.parse_args()

def firefox_android_repo(args):
    return f'https://github.com/{args.firefox_android_owner}/firefox-android'

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
