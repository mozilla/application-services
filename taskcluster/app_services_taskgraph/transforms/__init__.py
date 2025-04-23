# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import copy

from taskgraph.util.dependencies import group_by

from ..build_config import EXTENSIONS


def _extensions(type, secondary_extensions):
    primary_extensions = EXTENSIONS[type]
    return [
        package_ext + secondary_ext
        for package_ext in primary_extensions
        for secondary_ext in secondary_extensions
    ]


def _artifact_filename(name, version, extension):
    return f"{name}-{version}{extension}"


def publications_to_artifact_paths(version, publications, secondary_extensions=("",)):
    paths = []
    for publication in publications:
        for extension in _extensions(publication["type"], secondary_extensions):
            artifact_filename = _artifact_filename(
                publication["name"], version, extension
            )
            paths.append(f"public/build/{artifact_filename}")

    return paths


def publications_to_artifact_map_paths(
    version, publications, preview_build, secondary_extensions
):
    build_map_paths = {}
    for publication in publications:
        for extension in _extensions(publication["type"], secondary_extensions):
            publication_name = publication["name"]
            artifact_filename = _artifact_filename(publication_name, version, extension)
            if preview_build is not None:
                # Both nightly and other preview builds are places in separate directory
                destination = f"maven2/org/mozilla/appservices/nightly/{publication_name}/{version}/{artifact_filename}"
            else:
                destination = f"maven2/org/mozilla/appservices/{publication_name}/{version}/{artifact_filename}"
            build_map_paths[f"public/build/{artifact_filename}"] = {
                "checksums_path": "",  # XXX beetmover marks this as required, but it's not needed
                "destinations": [destination],
            }

    return build_map_paths


@group_by("component")
def component_grouping(config, tasks):
    """Custom group-by function for `from_deps` transforms"""
    groups = {}
    for task in tasks:
        if task.kind not in config.config.get("kind-dependencies", []):
            continue

        buildconfig = task.attributes["buildconfig"]
        component = buildconfig["name"]
        if component == "all":
            continue

        groups.setdefault(component, []).append(task)

    tasks_for_all_components = [
        task
        for task in tasks
        if task.attributes.get("buildconfig", {}).get("name", "") == "all"
    ]
    for _, grouped_tasks in groups.items():
        grouped_tasks.extend(copy.deepcopy(tasks_for_all_components))

    return groups.values()
