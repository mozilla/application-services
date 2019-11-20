# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals


def _extensions(type, secondary_extensions):
    primary_extensions = ('.pom', '.aar', '-sources.jar') if type == "aar" else ('.pom', '.jar')
    return [package_ext + secondary_ext for package_ext in primary_extensions for secondary_ext in secondary_extensions]


def _artifact_filename(name, version, extension):
    return "{}-{}{}".format(name, version, extension)


def publications_to_artifact_paths(name, version, publications, secondary_extensions=("",)):
    paths = []
    for publication in publications:
        for extension in _extensions(publication["type"], secondary_extensions):
            artifact_filename = _artifact_filename(name, version, extension)
            paths.append("public/build/{}".format(artifact_filename))

    return paths


def publications_to_artifact_map_paths(name, version, publications, secondary_extensions):
    build_map_paths = {}
    for publication in publications:
        for extension in _extensions(publication["type"], secondary_extensions):
            artifact_filename = _artifact_filename(name, version, extension)
            build_map_paths["public/build/{}".format(artifact_filename)] = {
                "checksums_path": "",  # TODO beetmover marks this as required, but it's not needed
                "destinations": ["maven2/org/mozilla/appservices/{}/{}/{}".format(name, version, artifact_filename)]
            }

    return build_map_paths
