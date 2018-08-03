# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""
Decision task for pull requests
"""

import datetime
import json
import os
import taskcluster

TASK_ID = os.environ.get('TASK_ID')
REPO_URL = os.environ.get('GITHUB_HEAD_REPO_URL')
BRANCH = os.environ.get('GITHUB_HEAD_BRANCH')
COMMIT = os.environ.get('GITHUB_HEAD_SHA')


def schedule_task(queue, taskId, task):
    print("TASK", taskId)
    print(json.dumps(task, indent=4, separators=(',', ': ')))

    result = queue.createTask(taskId, task)
    print("RESULT", taskId)
    print(json.dumps(result, indent=4, separators=(',', ': ')))


def create_fxaclient_task():
    created = datetime.datetime.now()
    expires = taskcluster.fromNow('1 year')
    deadline = taskcluster.fromNow('1 day')

    return {
        "workerType": 'github-worker',
        "taskGroupId": TASK_ID,
        "expires": taskcluster.stringDate(expires),
        "retries": 5,
        "created": taskcluster.stringDate(created),
        "tags": {},
        "priority": "lowest",
        "schedulerId": "taskcluster-github",
        "deadline": taskcluster.stringDate(deadline),
        "dependencies": [TASK_ID],
        "routes": [],
        "scopes": [],
        "requires": "all-completed",
        "payload": {
            "features": {},
            "maxRunTime": 7200,
            "image": "mozillamobile/rust-component:buildtools-27.0.3-ndk-r17b-ndk-version-26-rust-stable-rust-beta",
            "command": [
                "/bin/bash",
                "--login",
                "-cx",
                "export TERM=dumb && git clone %s && cd application-services && git fetch %s %s && git config advice.detachedHead false && git checkout %s && ./scripts/taskcluster-android.sh" % (REPO_URL, REPO_URL, BRANCH, COMMIT)
            ],
            "artifacts": {
                "public/bin/mozilla/fxa_client_android.zip": {
                    "type": "file",
                    "path": "/build/application-services/fxa-client/fxa_client_android.zip",
                },
                "public/bin/mozilla/fxa_client_android_deps.zip": {
                    "type": "file",
                    "path": "/build/application-services/fxa-client-deps/fxa_client_android_deps.zip",
                },
                "public/bin/mozilla/logins_android_deps.zip": {
                    "type": "file",
                    "path": "/build/application-services/logins-deps/logins_android_deps.zip",
                },
            },
            "deadline": taskcluster.stringDate(deadline)
        },
        "provisionerId": "aws-provisioner-v1",
        "metadata": {
            "name": "application-services - FxA client library",
            "description": "Building FxA client Rust library and native code dependencies",
            "owner": "nalexander@mozilla.com",
            "source": "https://github.com/mozilla/application-services"
        }
    }


if __name__ == "__main__":
    queue = taskcluster.Queue({'baseUrl': 'http://taskcluster/queue/v1'})

    schedule_task(queue, taskcluster.slugId(), create_fxaclient_task())
