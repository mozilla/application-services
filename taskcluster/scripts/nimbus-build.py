#!/usr/bin/env python3

import argparse
import subprocess
import pathlib
import os

# Repository root dir

def main():
    args = parse_args()
    binary = args.binary
    target = args.target
    os.makedirs(args.out_dir, exist_ok=True)
    filename = f'{binary}.exe' if '-windows-' in target else binary

    env = os.environ

    if target == 'aarch64-unknown-linux-gnu':
        env = os.environ.copy()
        env['RUSTFLAGS'] = '-C linker=aarch64-linux-gnu-gcc'

    subprocess.check_call([
        'cargo', 'build', '--bin', binary, '--release', '--target', target,
    ], env=env)
    subprocess.check_call([
        'zip', '-r', f'../build/{binary}-{target}.zip',
        pathlib.Path(target).joinpath('release', filename),
    ], cwd='target')

def parse_args():
    parser = argparse.ArgumentParser(prog='nimbus-build.py')
    parser.add_argument('out_dir', type=pathlib.Path)
    parser.add_argument('binary')
    parser.add_argument('target')
    return parser.parse_args()

if __name__ == '__main__':
    main()
