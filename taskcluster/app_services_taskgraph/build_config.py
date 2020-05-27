# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

import os
import yaml

from taskgraph.util.memoize import memoize


EXTENSIONS = {
    'aar': ('.aar', '.pom', '-sources.jar'),
    'jar': ('.jar', '.pom')

}
CHECKSUMS_EXTENSIONS = ('.sha1', '.md5')


def get_components():
    build_config = _read_build_config()
    return [{
        'name': name,
        'path': project['path'],
    } for (name, project) in build_config['projects'].items()]


def get_version():
    return _read_build_config()["libraryVersion"]


def get_extensions(module_name):
    publications = _read_build_config()["projects"][module_name]['publications']
    extensions = {}
    for publication in publications:
        artifact_type = publication['type']
        if artifact_type not in EXTENSIONS:
            raise ValueError(
                "For '{}', 'publication->type' must be one of {}".format(
                    module_name, repr(EXTENSIONS.keys())
                )
            )
        extensions[publication['name']] = [
                extension + checksum_extension
                for extension in EXTENSIONS[artifact_type]
                for checksum_extension in ('',) + CHECKSUMS_EXTENSIONS
        ]
    return extensions


@memoize
def _read_build_config():
    current_dir = os.path.dirname(os.path.realpath(__file__))
    project_dir = os.path.realpath(os.path.join(current_dir, '..', '..'))

    with open(os.path.join(project_dir, '.buildconfig-android.yml'), 'rb') as f:
        return yaml.safe_load(f)
