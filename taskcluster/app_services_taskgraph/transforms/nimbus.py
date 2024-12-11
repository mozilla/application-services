# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from collections import namedtuple

from taskgraph.transforms.base import TransformSequence

LINUX_BUILD_TARGETS = (
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-gnu",
)

MAC_BUILD_TARGETS = (
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)

# Transform for the nimbus-build tasks
build = TransformSequence()


@build.add
def setup_build_tasks(config, tasks):
    for task in tasks:
        binary = task["attributes"]["binary"]
        target = task["attributes"]["target"]
        if target in LINUX_BUILD_TARGETS:
            setup_linux_build_task(task, target, binary)
        elif target in MAC_BUILD_TARGETS:
            setup_mac_build_task(task, target, binary)
        else:
            raise ValueError(f"Unknown target for nimbus build task: {target}")
        yield task


def setup_linux_build_task(task, target, binary):
    docker_image = "linux"

    if target in ("aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"):
        docker_image = "linux2004"

    task["description"] = f"Build {binary} ({target})"
    task["worker-type"] = "b-linux"
    task["worker"] = {
        "max-run-time": 1800,
        "docker-image": {"in-tree": docker_image},
        "artifacts": [
            {
                "name": f"public/build/{binary}-{target}.zip",
                "path": f"/builds/worker/checkouts/vcs/build/{binary}-{target}.zip",
                "type": "file",
            }
        ],
    }
    task["run"] = {
        "using": "run-commands",
        "pre-commands": [
            ["git", "submodule", "update", "--init"],
            ["source", "taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh"],
        ],
        "commands": [
            ["taskcluster/scripts/nimbus-build.py", "build/", binary, target],
        ],
        "use-caches": True,
    }
    task["fetches"] = {
        "toolchain": [
            "rust",
        ]
    }


def setup_mac_build_task(task, target, binary):
    task["description"] = f"Build {binary} ({target})"
    task["worker-type"] = "b-osx"
    task["worker"] = {
        "max-run-time": 1800,
        "artifacts": [
            {
                "name": f"public/build/{binary}-{target}.zip",
                "path": f"checkouts/vcs/build/{binary}-{target}.zip",
                "type": "file",
            }
        ],
    }
    task["run"] = {
        "using": "run-commands",
        "run-task-command": ["/usr/local/bin/python3", "run-task"],
        "pre-commands": [
            ["source", "taskcluster/scripts/setup-mac-worker.sh"],
            ["source", "taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh"],
        ],
        "commands": [
            ["taskcluster/scripts/nimbus-build-osx.sh", "build/", binary, target]
        ],
    }
    task["fetches"] = {
        "toolchain": [
            "rust-osx",
        ]
    }


# Transform for the nimbus-assemble task
#
# This task produces a single zip file + checksum that combines the binaries from each individual
# build task.
assemble = TransformSequence()

# nimbus-build task that a nimbus-binaries-assemble task depends on
NimbusBuildDep = namedtuple("NimbusBuildDep", "label target")


@assemble.add
def setup_assemble_tasks(config, tasks):
    for task in tasks:
        # Which nimbus binary are we assembling?
        binary = task["attributes"]["nimbus-binary"]

        # Find nimbus-build task dependencies for our binary.
        build_task_deps = [
            NimbusBuildDep(label, build_task.attributes["target"])
            for (label, build_task) in config.kind_dependencies_tasks.items()
            if build_task.kind == "nimbus-build"
            and build_task.attributes.get("binary") == binary
        ]

        task["dependencies"] = {dep.label: dep.label for dep in build_task_deps}
        task["fetches"] = {
            dep.label: [
                {
                    "artifact": f"{binary}-{dep.target}.zip",
                    "dest": binary,
                    "extract": True if binary == "nimbus-fml" else False,
                }
            ]
            for dep in build_task_deps
        }

        artifact_path = "/builds/worker/artifacts"
        if binary == "nimbus-fml":
            # For nimbus-fml, we zip all binaries together and include the sha256
            task["release-artifacts"] = [f"{binary}.{ext}" for ext in ("zip", "sha256")]

            task["run"] = {
                "using": "run-commands",
                "commands": [
                    ["mkdir", "-p", artifact_path],
                    ["cd", "/builds/worker/fetches/nimbus-fml"],
                    ["zip", f"{artifact_path}/nimbus-fml.zip", "-r", "."],
                    ["cd", artifact_path],
                    ["eval", "sha256sum", "nimbus-fml.zip", ">", "nimbus-fml.sha256"],
                ],
            }
        elif binary == "nimbus-cli":
            # For nimbus-cli, we just publish the binaries separately
            task["release-artifacts"] = [
                f"{binary}-{dep.target}.zip" for dep in build_task_deps
            ]
            # Publish a JSON file with information about the build
            task["release-artifacts"].append("nimbus-cli.json")

            sources = [
                f"/builds/worker/fetches/{binary}/{binary}-{dep.target}.zip"
                for dep in build_task_deps
            ]

            task["run"] = {
                "using": "run-commands",
                "commands": [
                    ["mkdir", "-p", artifact_path],
                    ["cp"] + sources + [artifact_path],
                    [
                        "taskcluster/scripts/generate-nimbus-cli-json.py",
                        f"{artifact_path}/nimbus-cli.json",
                    ],
                ],
            }

        yield task
