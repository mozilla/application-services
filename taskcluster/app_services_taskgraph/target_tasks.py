# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import json
import logging

from taskgraph.target_tasks import _target_task, filter_for_tasks_for
from taskgraph.util import taskcluster

logger = logging.getLogger(__name__)

@_target_task('pr-skip')
def target_tasks_pr_skip(full_task_graph, parameters, graph_config):
    return []

# Don't filter out any tasks. We use this for:
#   - Pushes to a release branch
#   - PRs with `[preview: (nightly|release)]`
#
# This runs the same tasks as `pr-full`, plus:
#  - build-summary, which sends a slack alert if the build fails
#  - release-publish, which creates the `release.json` or `nightly.json` artifact
@_target_task('full')
def target_tasks_release(full_task_graph, parameters, graph_config):
    return full_task_graph.tasks

@_target_task('nightly')
def target_tasks_nightly(full_task_graph, parameters, graph_config):
    head_rev = parameters["head_rev"]
    try:
        taskcluster.find_task_id(
            f'project.application-services.v2.branch.main.revision.'
            f'{head_rev}.taskgraph.decision-nightly')
    except BaseException:
        # No nightly decision task run for this commit, which is expected
        pass
    else:
        # We already ran the nightly decision task and tried to build the nightly.  Don't try again.
        logger.info(f"Nightly already ran for {head_rev}, skipping")
        return []
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


def filter_release_promotion(
    full_task_graph, filtered_for_candidates, shipping_phase
):
    def filter(task):
        # Include promotion tasks; these will be optimized out
        if task.label in filtered_for_candidates:
            return True

        if task.attributes.get("shipping_phase") == shipping_phase:
            return True

    return [
        label for label, task in full_task_graph.tasks.items()
        if filter(task)
    ]


@_target_task("promote")
def target_tasks_promote(full_task_graph, parameters, graph_config):
    return filter_release_promotion(
        full_task_graph,
        filtered_for_candidates=[],
        shipping_phase="promote",
    )


@_target_task("ship")
def target_tasks_ship(full_task_graph, parameters, graph_config):
    filtered_for_candidates = target_tasks_promote(
        full_task_graph,
        parameters,
        graph_config,
    )
    return filter_release_promotion(
        full_task_graph, filtered_for_candidates, shipping_phase="ship"
    )
