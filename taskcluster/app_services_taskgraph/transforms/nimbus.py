# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from taskgraph.transforms.base import TransformSequence

# Transform for the nimbus-build tasks
build = TransformSequence()

@build.add
def setup_build_tasks(config, tasks):
    for task in tasks:
        binary = task['attributes']['binary']
        target = task['attributes']['target']
        if target in ('x86_64-unknown-linux-gnu', 'x86_64-pc-windows-gnu'):
            setup_linux_build_task(task, target, binary)
        elif target in ('x86_64-apple-darwin', 'aarch64-apple-darwin'):
            setup_mac_build_task(task, target, binary)
        else:
            raise ValueError(f"Unknown target for nimbus build task: {target}")
        yield task

def setup_linux_build_task(task, target, binary):
    task['description'] = f'Build {binary} ({target})'
    task['worker-type'] = 'b-linux'
    task['worker'] = {
        'max-run-time': 1800,
        'docker-image': { 'in-tree': 'linux' },
        'artifacts': [
            {
                'name': f'public/build/{binary}-{target}.zip',
                'path': f'/builds/worker/checkouts/vcs/build/{binary}-{target}.zip',
                'type': 'file',
            }
        ]
    }
    task['run'] = {
        'using': 'run-commands',
        'pre-commands': [
            ['git', 'submodule', 'update', '--init'],
            ['source', 'taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh'],
        ],
        'commands': [
            ['taskcluster/scripts/nimbus-build.py', 'build/', binary, target],
        ],
        'use-caches': True,
    }
    task['fetches'] = {
        'toolchain': [
            'rust',
        ]
    }

def setup_mac_build_task(task, target, binary):
    task['description'] = f'Build {binary} ({target})'
    task['worker-type'] = 'b-osx'
    task['worker'] = {
        'max-run-time': 1800,
        'artifacts': [
            {
                'name': f'public/build/{binary}-{target}.zip',
                'path': f'checkouts/vcs/build/{binary}-{target}.zip',
                'type': 'file',
            }
        ]
    }
    task['run'] = {
        'using': 'run-commands',
        'run-task-command': ["/usr/local/bin/python3", "run-task"],
        'pre-commands': [
            ["taskcluster/scripts/toolchain/build-rust-toolchain-macosx.sh"],
            ["taskcluster/scripts/toolchain/libs-ios.sh"],
        ],
        'commands': [
            [ "taskcluster/scripts/nimbus-build-osx.sh", "build/", binary, target ]
        ],
    }

# Transform for the nimbus-assemble task
#
# This task produces a single zip file + checksum that combines the binaries from each individual
# build task.
assemble = TransformSequence()

@assemble.add
def setup_assemble_tasks(config, tasks):
    binaries = [
        'nimbus-fml',
        'nimbus-cli',
    ]
    for task in tasks:
        dependencies = {}
        fetches = {}

        for (label, build_task) in config.kind_dependencies_tasks.items():
            if build_task.kind != "nimbus-build":
                continue

            # Since `nimbus-build` is listed in kind-dependencies, this loops through all the
            # nimbus-build tasks.  For each task, fetch it's artifacts.
            binary = build_task.attributes['binary']
            target = build_task.attributes['target']
            dependencies[label] = label
            fetches[label] = [
                {
                    'artifact': f'{binary}-{target}.zip',
                    'dest': binary,
                    'extract': True,
                }
            ]
        task['dependencies'] = dependencies
        task['fetches'] = fetches

        # For each nimbus binary, the `assemble-nimbus-binaries.sh` script will combine the binary from
        # each of the individual tasks into a single zipfile with all artifacts (plus a checksum
        # file).  Output these as artifacts.
        task['worker']['artifacts'] = [
            {
                'name': f'public/build/{binary}.{ext}',
                'path': f'/builds/worker/checkouts/vcs/build/{binary}.{ext}',
                'type': 'file',
            }
            for binary in binaries
            for ext in ('zip', 'sha256')
        ]

        yield task
