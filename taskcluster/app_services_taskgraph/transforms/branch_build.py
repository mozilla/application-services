# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from copy import deepcopy
import os
import json

from taskgraph.transforms.base import TransformSequence
from taskgraph.util.schema import validate_schema, Schema
from voluptuous import Optional, Required, In

TASKCLUSTER_DIR = os.path.dirname(os.path.dirname(os.path.dirname(__file__)))

# Schema for the job dictionary from kinds.yml
branch_build_schema = Schema({
  # Which repository are we working on
  Required("repository"): In([
      'application-services',
      'firefox-android',
      'fenix',
  ]),
  # Name of the task, taken from the key in the tasks dict.  This determines
  # what operation we are going to perform
  Required('name'): In(['build', 'test']),
  # Keys used by taskcluster
  Optional("task-from"): str,

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
            'robolectric',
            'rust',
        ],
    },
    'run': {
        'using': 'gradlew',
        'pre-gradlew': [
            ["git", "submodule", "update", "--init"],
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

    for task in tasks:
        repo_name = task.pop('repository')
        operation = task['name']
        task.update(deepcopy(TASK_COMMON))
        task['description'] = '{} {}'.format(operation, repo_name)
        if repo_name == 'application-services':
            setup_application_services(task)
        elif repo_name == 'firefox-android':
            setup_firefox_android(task, branch_build_params)
        elif repo_name == 'fenix':
            setup_fenix(task, branch_build_params)
        else:
            raise ValueError("Invalid branch build repository: {}".format(repo_name))

        if operation == 'build':
            for task in get_build_tasks(task, repo_name):
                yield task
        elif operation == 'test':
            for task in get_test_tasks(task, repo_name):
                yield task
        else:
            raise ValueError("Invalid branch build operation: {}".format(operation))

def setup_application_services(task):
    task['run']['pre-gradlew'].extend([
        ["source", "taskcluster/scripts/toolchain/setup-fetched-rust-toolchain.sh"],
        ["rsync", "-a", "/builds/worker/fetches/libs/", "/builds/worker/checkouts/vcs/libs/"],
        ["taskcluster/scripts/setup-branch-build.py"],
    ])
    task['fetches'] = {
        'toolchain': [
            'android-libs',
            'desktop-linux-libs',
            'desktop-macos-libs',
            'rust',
        ],
    }

def setup_firefox_android(task, branch_build_params):
    task['dependencies'] = {
        'branch-build-as': 'branch-build-as-build',
    }

    task['fetches'] = {
        'toolchain': [
            'robolectric',
        ],
        'branch-build-as': [ 'application-services-m2.tar.gz' ],
    }
    task['run']['pre-gradlew'].extend([
        ['rsync', '-a', '/builds/worker/fetches/.m2/', '/builds/worker/.m2/'],
        setup_branch_build_command_line(branch_build_params, setup_fenix=False),
        ['cd', 'firefox-android/android-components'],
        ['git', 'rev-parse', '--short', 'HEAD'],
        # Building this up-front seems to make the build more stable.  I think
        # having multiple components all try to execute the
        # Bootstrap_CONDA_'Miniconda3' task in parallel causes issues.
        ['./gradlew', ":browser-engine-gecko:Bootstrap_CONDA_'Miniconda3'"],
    ])

def setup_fenix(task, branch_build_params):
    task['dependencies'] = {
        'branch-build-as': 'branch-build-as-build',
        'branch-build-firefox-android': 'branch-build-firefox-android-build',
    }

    task['fetches'] = {
        'toolchain': [
            'robolectric',
        ],
        'branch-build-as': [ 'application-services-m2.tar.gz' ],
        'branch-build-firefox-android': ['firefox-android-m2.tar.gz' ],
    }
    task['run']['pre-gradlew'].extend([
        ['rsync', '-a', '/builds/worker/fetches/.m2/', '/builds/worker/.m2/'],
        setup_branch_build_command_line(branch_build_params, setup_fenix=True),
        ['cd', 'fenix'],
        ['git', 'rev-parse', '--short', 'HEAD'],
    ])

def setup_branch_build_command_line(branch_build_params, setup_fenix):
    cmd_line = [
            'taskcluster/scripts/setup-branch-build.py',
            '--firefox-android-owner',
            branch_build_params.get('firefox-android-owner', 'mozilla-mobile'),
            '--firefox-android-branch',
            branch_build_params.get('firefox-android-branch', 'main'),
    ]
    if setup_fenix:
        cmd_line.extend([
                '--fenix-owner',
                branch_build_params.get('fenix-owner', 'mozilla-mobile'),
                '--fenix-branch',
                branch_build_params.get('fenix-branch', 'main'),
        ])
    return cmd_line

def get_build_tasks(task, repo_name):
    if repo_name == 'fenix':
        return get_fenix_build_tasks(task)
    else:
        return get_maven_package_build_tasks(task, repo_name)

def get_maven_package_build_tasks(task, repo_name):
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
    yield task

def get_fenix_build_tasks(task):
    task['run']['gradlew'] = ['assembleDebug']
    task['worker']['artifacts'] = [
        {
            'name': 'public/branch-build/app-x86-debug.apk',
            'path': '/builds/worker/checkouts/vcs/fenix/app/build/outputs/apk/debug/app-x86-debug.apk',
            'type': 'file',
        }
    ]
    yield task

def get_test_tasks(task, repo_name):
    if repo_name == 'firefox-android':
        # Split up the android components tasks by project.  Running them all at once in the same task tends to cause failures
        # Also, use `testRelease` instead of `testDebugUnitTest`.  I'm not sure what the difference is, but this is what the android-components CI runs.
        for project in get_android_components_projects():
            project_task = deepcopy(task)
            project_task['description'] += ' {}'.format(project)
            project_task['run']['gradlew'] = android_components_test_gradle_tasks(project)
            project_task['name'] = 'test-{}'.format(project)
            yield project_task
    else:
        task['run']['gradlew'] = ['testDebugUnitTest']
        yield task

def android_components_test_gradle_tasks(project):
    # Gradle tasks to run to test an android-components project, this should
    # match the android-components code in `taskcluster/ci/build/kind.yml`
    if project == 'tooling-lint':
        tasks = [
            ':{project}:assemble',
            ':{project}:assembleAndroidTest',
            ':{project}:test',
            ':{project}:lint',
            'githubBuildDetails',
        ]
    elif project == 'tooling-detekt':
        tasks = [
            ':{project}:assemble',
            ':{project}:assembleAndroidTest',
            ':{project}:test',
            ':{project}:lintRelease',
            'githubBuildDetails',
        ]
    else:
        tasks = [
            ':{project}:assemble',
            ':{project}:assembleAndroidTest',
            ':{project}:testRelease',
            ':{project}:lintRelease',
            'githubBuildDetails',
        ]
    return [t.format(project=project) for t in tasks]

def get_android_components_projects():
    path = os.path.join(TASKCLUSTER_DIR, 'android-components-projects.json')
    with open(path) as f:
        return json.load(f)

