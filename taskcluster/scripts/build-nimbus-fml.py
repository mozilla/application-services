#!/usr/bin/python3

import argparse
import subprocess
import pathlib
import os

# Repository root dir
ROOT_DIR = pathlib.Path(__file__).parent.parent.parent

def main():
    args = parse_args()
    if not args.out_dir.exists():
        os.makedirs(args.out_dir)
    # TODO: implement this for real once we have access to a Mac worker on taskcluster
    subprocess.check_call([
        'curl', '-LsS',
        '-o', str(args.out_dir / 'nimbus-fml.zip'),
        'https://github.com/mozilla/application-services/releases/download/v97.2.0/nimbus-fml.zip',
    ])
    subprocess.check_call([
        'curl', '-LsS',
        '-o', str(args.out_dir / 'nimbus-fml.sha256'),
        'https://github.com/mozilla/application-services/releases/download/v97.2.0/nimbus-fml.sha256',
    ])

def parse_args():
    parser = argparse.ArgumentParser(prog='build-and-test-swift.py')
    parser.add_argument('out_dir', type=pathlib.Path)
    return parser.parse_args()

if __name__ == '__main__':
    main()
