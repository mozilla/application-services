# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

from taskgraph.transforms.base import TransformSequence


transforms = TransformSequence()


@transforms.add
def release_upload_symbols(config, tasks):
    for task in tasks:
        if config.params["tasks_for"] != "github-release":
            yield task
            return

        task["worker"]["script"] += "./automation/upload_android_symbols.sh {}".format(task["attributes"]["buildconfig"]["path"])
        task["scopes"].append("secrets:get:project/application-services/symbols-token")
        yield task


@transforms.add
def build_task(config, tasks):
    for task in tasks:
        module_info = task["attributes"]["buildconfig"]
        name = module_info["name"]
        version = module_info["version"]

        script = task["worker"].pop("script").format(module_name=name)
        bash_command = [
            "/bin/bash",
            "--login",
            "-c",
            "cat <<'SCRIPT' > ../script.sh && bash -e ../script.sh\n{}\nSCRIPT".format(script)
        ]

        task["run"]["command"] = bash_command
        task["description"] = task["description"].format(module_name=name)
        task["worker"]["artifacts"] = artifacts = []

        for publication in module_info["publications"]:
            primary_extensions = (".pom", ".aar", "-sources.jar") if publication["type"] == "aar" else (".pom", ".jar")
            extensions = [package_ext + digest_ext for package_ext in primary_extensions for digest_ext in ("", ".sha1", ".md5")]
            for extension in extensions:
                artifact_filename = "{}-{}{}".format(name, version, extension)
                artifacts.append({
                    "name": "public/build/{}".format(artifact_filename),
                    "path": "/builds/worker/checkouts/src/build/maven/org/mozilla/appservices/{}/{}/{}".format(name, version, artifact_filename),
                    "type": "file",
                })


        yield task
