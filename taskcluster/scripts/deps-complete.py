#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

import json
import os
import sys
from urllib.request import urlopen

TASKCLUSTER_PROXY_URL = os.environ["TASKCLUSTER_PROXY_URL"]
DECISION_TASK_ID = os.environ["DECISION_TASK_ID"]
TASK_ID = os.environ["TASK_ID"]


def get_tasks_from_group(task_group_id):
    continuation_token = None
    tasks = []
    while True:
        url = f"{TASKCLUSTER_PROXY_URL}/queue/v1/task-group/{task_group_id}/list"
        if continuation_token:
            url += f"?continuationToken={continuation_token}"
        data = json.load(urlopen(url))
        tasks.extend(data["tasks"])
        continuation_token = data.get("continuationToken")
        if continuation_token is None:
            break
    return tasks


def get_dependent_task_data():
    task_map = {
        t["status"]["taskId"]: t for t in get_tasks_from_group(DECISION_TASK_ID)
    }
    dependency_ids = task_map.get(TASK_ID)["task"]["dependencies"]
    return [
        task_map[task_id]
        for task_id in dependency_ids
        # Missing keys indicate cached dependencies, which won't be included in
        # the task group data
        if task_id in task_map
    ]


def check_dependent_tasks():
    some_task_failed = False
    for task in get_dependent_task_data():
        if task["status"]["state"] != "completed":
            some_task_failed = True
            name = task["task"]["metadata"]["name"]
            print(f"Failed task: {name}")
    return some_task_failed


def main():
    print()
    print("---- Checking for failed tasks ----")
    if check_dependent_tasks():
        sys.exit(1)
    else:
        print("All successful!")
        sys.exit(0)


if __name__ == "__main__":
    main()
