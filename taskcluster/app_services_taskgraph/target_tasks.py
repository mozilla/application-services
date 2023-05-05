# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from taskgraph.target_tasks import _target_task, filter_for_tasks_for


@_target_task('pr-skip')
def target_tasks_pr_skip(full_task_graph, parameters, graph_config):
    return []

# Don't filter out any tasks. We use this for:
#   - Pushes to a release branch
#   - The nightly cron task
#   - PRs with `[preview: (nightly|release)]`
#
# This runs the same tasks as `pr-full`, plus:
#  - build-summary, which sends a slack alert if the build fails
#  - release-publish, which creates the `release.json` or `nightly.json` artifact
@_target_task('full')
def target_tasks_release(full_task_graph, parameters, graph_config):
    return full_task_graph.tasks

@_target_task('pr-full')
def target_tasks_all(full_task_graph, parameters, graph_config):
    """Target the tasks which have indicated they should be run on this project
    via the `run_on_projects` attributes."""
    def filter(task):
        return (filter_for_tasks_for(task, parameters) 
                and task.attributes.get("run-on-pr-type", "all") in ("full-ci", "all")
                and task.attributes.get('release-type') != 'release-only')

    return [l for l, task in full_task_graph.tasks.items() if filter(task)]

@_target_task('pr-normal')
def target_tasks_default(full_task_graph, parameters, graph_config):
    """Target the tasks which have indicated they should be run on this project
    via the `run_on_projects` attributes."""
    def filter(task):
        return (filter_for_tasks_for(task, parameters)
                and task.attributes.get("run-on-pr-type", "all") in ("normal-ci", "all")
                and task.attributes.get('release-type') != 'release-only')

    return [l for l, task in full_task_graph.tasks.items() if filter(task)]
