# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import os
import re

from taskgraph.filter_tasks import filter_task

REPO_RE = r"((?P<owner>[\.\w-]+)[/:])?(?P<branch>[\.\w-]+)"
FIREFOX_IOS_BRANCH_RE = re.compile(r"\[firefox-ios:\s*" + REPO_RE + r"\]")
FIREFOX_ANDROID_BRANCH_RE = re.compile(r"\[firefox-android:\s*" + REPO_RE + r"\]")


def calc_branch_build_param(parameters):
    title = os.environ.get("APPSERVICES_PULL_REQUEST_TITLE", "")
    branch_build = {}

    firefox_android_branch_match = FIREFOX_ANDROID_BRANCH_RE.search(title)
    if firefox_android_branch_match:
        branch_build["firefox-android"] = {
            "owner": calc_owner(firefox_android_branch_match),
            "branch": firefox_android_branch_match.group("branch"),
        }

    firefox_ios_branch_match = FIREFOX_IOS_BRANCH_RE.search(title)
    if firefox_ios_branch_match:
        branch_build["firefox-ios"] = {
            "owner": calc_owner(firefox_ios_branch_match),
            "branch": firefox_ios_branch_match.group("branch"),
        }

    return branch_build


def calc_owner(match):
    if match.group("owner"):
        return match.group("owner")
    else:
        return "mozilla-mobile"


@filter_task("branch-build")
def filter_branch_build_tasks(full_task_graph, parameters, graph_config):
    def should_keep_task(task):
        task_branch_build = task.attributes.get("branch-build")
        if task_branch_build is None:
            # Don't filter out tasks without a `branch-build` attribute
            return True
        else:
            # For tasks with a `branch-build` attribute, include them if there's a matching
            # parameter
            return task_branch_build in parameters.get("branch-build", {})

    return [l for l, task in full_task_graph.tasks.items() if should_keep_task(task)]
