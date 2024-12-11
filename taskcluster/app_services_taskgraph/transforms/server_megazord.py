# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from collections import namedtuple

from taskgraph.transforms.base import TransformSequence

# Transform for the nimbus-build tasks
build = TransformSequence()

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


@build.add
def setup_build_tasks(config, tasks):
    for task in tasks:
        binary = task["attributes"]["megazord"]
        target = task["attributes"]["target"]
        if target in LINUX_BUILD_TARGETS:
            setup_linux_build_task(task, target, binary)
        elif target in MAC_BUILD_TARGETS:
            setup_mac_build_task(task, target, binary)
        else:
            raise ValueError(f"Unknown target for nimbus build task: {target}")
        yield task


def setup_linux_build_task(task, target, binary):
    task["description"] = f"Build {binary} ({target})"
    task["worker-type"] = "b-linux"
    docker_image = "linux"
    if target in ("aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"):
        docker_image = "linux2004"
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
            ["taskcluster/scripts/server-megazord-build.py", binary, target, "build/"],
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
            ["taskcluster/scripts/server-megazord-build.py", binary, target, "build/"]
        ],
    }
    task["fetches"] = {
        "toolchain": [
            "rust-osx",
        ]
    }


# Transform for the server-megazord-assemble task
#
# This task produces a single zip file + checksum that combines the binaries from each individual
# build task.
assemble = TransformSequence()

# server-megazord task that a server-megazord-assemble task depends on
MegazordBuildDep = namedtuple("MegazordBuildDep", "label target")


@assemble.add
def setup_assemble_tasks(config, tasks):
    for task in tasks:
        # Which megazord binary are we assembling?
        binary = task["attributes"]["megazord"]

        # Find server-megazord-build task dependencies for our binary.
        build_task_deps = [
            MegazordBuildDep(label, build_task.attributes["target"])
            for (label, build_task) in config.kind_dependencies_tasks.items()
            if build_task.kind == "server-megazord-build"
            and build_task.attributes.get("megazord") == binary
        ]

        task["dependencies"] = {dep.label: dep.label for dep in build_task_deps}
        task["fetches"] = {
            dep.label: [
                {
                    "artifact": f"{binary}-{dep.target}.zip",
                    "dest": binary,
                    "extract": True,
                }
            ]
            for dep in build_task_deps
        }

        artifact_path = "/builds/worker/artifacts"

        # For server megazords, we zip all binaries together and include the sha256
        task["release-artifacts"] = [f"{binary}.{ext}" for ext in ("zip", "sha256")]

        task["run"] = {
            "using": "run-commands",
            "commands": [
                ["mkdir", "-p", artifact_path],
                ["cd", f"/builds/worker/fetches/{binary}"],
                ["zip", f"{artifact_path}/{binary}.zip", "-r", "."],
                ["cd", artifact_path],
                ["eval", "sha256sum", f"{binary}.zip", ">", f"{binary}.sha256"],
            ],
        }
        yield task
