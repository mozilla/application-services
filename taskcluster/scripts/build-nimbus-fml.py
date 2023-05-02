#!/usr/bin/python3

import argparse
import subprocess
import pathlib
import os

# Repository root dir
ROOT_DIR = pathlib.Path(__file__).parent.parent.parent
COMMAND = 'components/support/nimbus-fml/scripts/build-dist.sh'


def main():
    args = parse_args()
    if not args.out_dir.exists():
        os.makedirs(args.out_dir)
    subprocess.check_call([
        COMMAND
    ])

def parse_args():
    parser = argparse.ArgumentParser(prog='build-and-test-swift.py')
    parser.add_argument('out_dir', type=pathlib.Path)
    return parser.parse_args()

if __name__ == '__main__':
    main()
