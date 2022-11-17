#!/usr/bin/python3
#
# Update the projects list for android-components and Fenix.  This is required
# for branch builds to work properly.  Run this whenever a new project is
# added.

import argparse
import json
import os
import yaml

TASKCLUSTER_DIR = os.path.dirname(os.path.dirname(__file__))
PROJECTS_FILENAME = "android-components-projects.json"
# Don't try to run tests for these
EXCLUDED_PROJECTS = set([
    'samples-browser',
])

def main():
    args = parse_args()
    ac_projects = get_android_components_projects(args)
    write_projects(ac_projects)
    print("{} updated".format(PROJECTS_FILENAME))

def parse_args():
    parser = argparse.ArgumentParser(
        description='Update the component list for android-components and fenix')

    parser.add_argument('android_components_dir')
    return parser.parse_args()

def get_android_components_projects(args):
    path = os.path.join(args.android_components_dir, ".buildconfig.yml")
    with open(path) as f:
        build_config = yaml.safe_load(f.read())
    return [
        p for p in build_config['projects'].keys()
        if p not in EXCLUDED_PROJECTS
    ]

def write_projects(ac_projects):
    path = os.path.join(TASKCLUSTER_DIR, PROJECTS_FILENAME)
    with open(path, "wt") as f:
        json.dump(ac_projects, f, sort_keys=True, indent=4)

if __name__ == '__main__':
    main()
