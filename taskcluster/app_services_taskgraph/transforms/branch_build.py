# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from copy import deepcopy

from taskgraph.transforms.base import TransformSequence
from taskgraph.util.schema import validate_schema, Schema
from voluptuous import Optional, Required, In

# Schema for the job dictionary from kinds.yml
branch_build_schema = Schema({
  # Which repository are we working on
  Required("repository"): In([
      'application-services',
      'android-components',
      'fenix',
  ]),
  # Name of the task, taken from the key in the jobs dict.  This determines
  # what operation we are going to perform
  Required('name'): In(['build', 'test']),
  # Keys used by taskcluster
  Optional("job-from"): str,

})

transforms = TransformSequence()

# Shared task attributes
TASK_COMMON = {
    'attributes': {
        'artifact_prefix': 'public/branch-build',
        'resource-monitor': True,
        'branch-build': True,
    },
    'worker-type': 'b-linux',
    'worker': {
        'chain-of-trust': True,
        'docker-image': { 'in-tree': 'linux' },
        'max-run-time': 1800,
    },
    'fetches': {
        'toolchain': [
            'android-libs',
            'desktop-linux-libs',
            'desktop-macos-libs',
            'desktop-win32-x86-64-libs',
            'rust',
        ],
    },
    'run': {
        'using': 'gradlew',
        'pre-gradlew': [
            ["git", "submodule", "update", "--init"],
            ["source", "taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh"],
            ["rsync", "-a", "/builds/worker/fetches/libs/", "/builds/worker/checkouts/vcs/libs/"],
        ]
    },
}

@transforms.add
def validate(config, tasks):
    for task in tasks:
        validate_schema(
            branch_build_schema,
            task,
            "In branch build task {!r}:".format(task.get("name", "<unknown>")),
        )
        yield task

@transforms.add
def setup(config, tasks):
    branch_build_params = config.params.get('branch-build', {})
    android_components_branch = branch_build_params.get('android-components-branch', 'main')
    fenix_branch = branch_build_params.get('fenix-branch', 'main')

    for task in tasks:
        repo_name = task.pop('repository')
        operation = task['name']
        task.update(deepcopy(TASK_COMMON))
        if repo_name == 'application-services':
            setup_application_services(task)
        elif repo_name == 'android-components':
            setup_android_components(task, android_components_branch)
        elif repo_name == 'fenix':
            setup_fenix(task, android_components_branch, fenix_branch)
        else:
            raise ValueError("Invalid branch build repository: {}".format(repo_name))

        if operation == 'build':
            setup_build(task, repo_name)
        elif operation == 'test':
            setup_test(task, repo_name)
        else:
            raise ValueError("Invalid branch build operation: {}".format(operation))
        task['description'] = '{} {}'.format(operation, repo_name)
        yield task

def setup_application_services(task):
    task['run']['pre-gradlew'].extend([
        ["taskcluster/scripts/setup-branch-build.py"],
    ])

def setup_android_components(task, android_components_branch):
    task['dependencies'] = {
        'branch-build-as': 'branch-build-as-build',
    }

    task['fetches']['branch-build-as'] = [ 'application-services-m2.tar.gz' ]
    task['run']['pre-gradlew'].extend([
        ['rsync', '-a', '/builds/worker/fetches/.m2/', '/builds/worker/.m2/'],
        [
            "taskcluster/scripts/setup-branch-build.py",
            '--android-components', android_components_branch,
        ],
        ['cd', 'android-components'],
        # Building this up-front seems to make the build more stable.  I think
        # having multiple components all try to execute the
        # Bootstrap_CONDA_'Miniconda3' task in parallel causes issues.
        ['./gradlew', ":browser-engine-gecko:Bootstrap_CONDA_'Miniconda3'"],
    ])

def setup_fenix(task, android_components_branch, fenix_branch):
    task['dependencies'] = {
        'branch-build-as': 'branch-build-as-build',
        'branch-build-ac': 'branch-build-ac-build',
    }

    task['fetches']['branch-build-as'] = ['application-services-m2.tar.gz' ]
    task['fetches']['branch-build-ac'] = ['android-components-m2.tar.gz' ]
    task['run']['pre-gradlew'].extend([
        ['rsync', '-a', '/builds/worker/fetches/.m2/', '/builds/worker/.m2/'],
        [
            "taskcluster/scripts/setup-branch-build.py",
            '--android-components', android_components_branch,
            '--fenix', fenix_branch,
        ],
        ['cd', 'fenix'],
    ])

def setup_build(task, repo_name):
    if repo_name == 'fenix':
        setup_fenix_build(task)
    else:
        setup_maven_package_build(task, repo_name)

def setup_maven_package_build(task, repo_name):
    task['run']['gradlew'] = ['publishToMavenLocal']
    task['run']['post-gradlew'] = [
        [
            'tar', 'zc',
            '--directory=/builds/worker/',
            '--file=/builds/worker/artifacts/m2-mozilla.tar.gz',
            '.m2/repository/org/mozilla/',
        ],
    ]
    task['worker']['artifacts'] = [
        {
            'name': 'public/branch-build/{}-m2.tar.gz'.format(repo_name),
            'path': '/builds/worker/artifacts/m2-mozilla.tar.gz',
            'type': 'file',
        }
    ]

def setup_fenix_build(task):
    task['run']['gradlew'] = ['assembleDebug']
    task['worker']['artifacts'] = [
        {
            'name': 'public/branch-build/app-x86-debug.apk',
            'path': '/builds/worker/checkouts/vcs/fenix/app/build/outputs/apk/debug/app-x86-debug.apk',
            'type': 'file',
        }
    ]

def setup_test(task, repo_name):
    task['run']['gradlew'] = ['testDebugUnitTest']
