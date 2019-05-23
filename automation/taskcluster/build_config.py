# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import print_function

import itertools
import os
from enum import Enum

import yaml

cached_build_config = None

def read_build_config():
    global cached_build_config

    if cached_build_config is None:
        with open(os.path.join(os.path.dirname(__file__), '..', '..', '.buildconfig-android.yml'), 'rb') as f:
            cached_build_config = yaml.safe_load(f)
    return cached_build_config


class ArtifactType(Enum):
    AAR = 'aar'
    JAR = 'jar'


def module_definitions():
    build_config = read_build_config()
    version = build_config['libraryVersion']
    modules_defs = []
    for (name, project) in build_config['projects'].items():
        project_path = os.path.abspath(project['path'])
        module_artifacts = []
        for artifact in project['publishedArtifacts']:
            artifact_name = artifact['name']
            artifact_type = ArtifactType(artifact['type'])

            extensions = ('.pom', '.aar', '-sources.jar') if artifact_type == ArtifactType.AAR else ('.pom', '.jar')
            extensions = [package_ext + digest_ext for package_ext, digest_ext in itertools.product(extensions, ('', '.sha1', '.md5'))]
            for extension in extensions:
                artifact_filename = '{}-{}{}'.format(artifact_name, version, extension)
                filename_with_package = f'org/mozilla/appservices/{artifact_name}/{version}/{artifact_filename}'
                module_artifacts.append({
                    'taskcluster_path': f'public/build/{artifact_filename}',
                    'build_fs_path': f'{project_path}/build/maven/{filename_with_package}',
                    'maven_destination': f'maven2/{artifact_name}'
                })


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
