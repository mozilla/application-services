# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import print_function
import os
import yaml

cached_build_config = None

def read_build_config():
    global cached_build_config

    if cached_build_config is None:
        with open(os.path.join(os.path.dirname(__file__), '..', '..', '.buildconfig-android.yml'), 'rb') as f:
            cached_build_config = yaml.safe_load(f)
    return cached_build_config


def module_definitions():
    build_config = read_build_config()
    modules_defs = []
    for (name, project) in build_config['projects'].items():
        module_artifacts = [{
            'name': published_artifact,
            # We hardcode the base directory because the decision task and the build task don't have
            # the same cwd.
            'artifact': '{}/{}/build/{}.maven.zip'.format("/build/repo", project['path'], published_artifact),
            'path': 'public/{}.maven.zip'.format(published_artifact),
        } for published_artifact in project["publishedArtifacts"]]
        modules_defs.append({
            'name': name,
            'artifacts': module_artifacts,
            'uploadSymbols': project.get('uploadSymbols', False),
            'path': project['path'],
        })
    return modules_defs

def appservices_version():
    build_config = read_build_config()
    return build_config['libraryVersion']
