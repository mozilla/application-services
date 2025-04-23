# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


import functools
import os

import yaml

EXTENSIONS = {"aar": (".aar", ".pom", "-sources.jar"), "jar": (".jar", ".pom")}
CHECKSUMS_EXTENSIONS = (".sha1", ".md5")


def get_components():
    build_config = _read_build_config()
    return [
        {
            "name": name,
            "path": project["path"],
            "artifactId": project["artifactId"],
            "uploadSymbols": project.get("uploadSymbols"),
            "publications": [
                {
                    "name": publication["name"],
                    "type": publication["type"],
                }
                for publication in project["publications"]
            ],
        }
        for (name, project) in build_config["projects"].items()
    ]


def get_version(params):
    version = get_version_from_version_txt()
    preview_build = params.get("preview-build")
    if preview_build == "nightly":
        components = version.split(".")
        assert len(components) == 2
        components[1] = params["moz_build_date"]
        return ".".join(components)
    elif preview_build is not None:
        raise NotImplementedError("Only nightly preview builds are currently supported")
    else:
        return version


@functools.cache
def get_version_from_version_txt():
    current_dir = os.path.dirname(os.path.realpath(__file__))
    project_dir = os.path.realpath(os.path.join(current_dir, "..", ".."))

    with open(os.path.join(project_dir, "version.txt")) as f:
        return f.read().strip()


def get_extensions(module_name):
    publications = _read_build_config()["projects"][module_name]["publications"]
    extensions = {}
    for publication in publications:
        artifact_type = publication["type"]
        if artifact_type not in EXTENSIONS:
            raise ValueError(
                f"For '{module_name}', 'publication->type' must be one of {repr(EXTENSIONS.keys())}"
            )
        extensions[publication["name"]] = [
            extension + checksum_extension
            for extension in EXTENSIONS[artifact_type]
            for checksum_extension in ("",) + CHECKSUMS_EXTENSIONS
        ]
    return extensions


@functools.cache
def _read_build_config():
    current_dir = os.path.dirname(os.path.realpath(__file__))
    project_dir = os.path.realpath(os.path.join(current_dir, "..", ".."))

    with open(os.path.join(project_dir, ".buildconfig-android.yml"), "rb") as f:
        return yaml.safe_load(f)
