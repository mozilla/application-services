# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

import os
import yaml

from taskgraph.util.memoize import memoize


def get_components():
    build_config = _read_build_config()
    return [{
        'version': build_config['libraryVersion'],
        'name': name,
        'path': project['path'],
        'artifactId': project['artifactId'],
        'publications': [{
            'name': publication['name'],
            'type': publication['type'],
        } for publication in project['publications']]
    } for (name, project) in build_config['projects'].items()]


@memoize
def _read_build_config():
    current_dir = os.path.dirname(os.path.realpath(__file__))
    project_dir = os.path.realpath(os.path.join(current_dir, '..', '..'))

    with open(os.path.join(project_dir, '.buildconfig-android.yml'), 'rb') as f:
        return yaml.safe_load(f)
