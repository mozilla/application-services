# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import logging

from taskgraph.target_tasks import filter_for_tasks_for, register_target_task
from taskgraph.util import taskcluster

logger = logging.getLogger(__name__)


def filter_out_shipping_phase(task):
    """Return False for "release" tasks, i.e. tasks with a promote/ship
    "shipping_phase" attribute unless they also have the "nightly" attribute.
    """
    return task.attributes.get("nightly") or task.attributes.get("shipping_phase") in {
        None,
        "build",
    }


@register_target_task("pr-skip")
def target_tasks_pr_skip(full_task_graph, parameters, graph_config):
    return []


# Don't filter out any tasks. We use this for:
#   - Pushes to a release branch
#   - PRs with `[preview: (nightly|release)]`
#
# This runs the same tasks as `pr-full`, plus:
#  - build-summary, which sends a slack alert if the build fails
#  - release-publish, which creates the `release.json` or `nightly.json` artifact
@register_target_task("full")
def target_tasks_release(full_task_graph, parameters, graph_config):
    return full_task_graph.tasks


@register_target_task("nightly")
def target_tasks_nightly(full_task_graph, parameters, graph_config):
    head_rev = parameters["head_rev"]
    try:
        taskcluster.find_task_id(
            f"project.application-services.v2.branch.main.revision."
            f"{head_rev}.taskgraph.decision-nightly"
        )
    except BaseException:
        # No nightly decision task run for this commit, which is expected
        pass
    else:
        # We already ran the nightly decision task and tried to build the nightly.  Don't try again.
        logger.info(f"Nightly already ran for {head_rev}, skipping")
        return []
    return [
        l
        for l, task in full_task_graph.tasks.items()
        if filter_out_shipping_phase(task)
    ]


@register_target_task("pr-full")
def target_tasks_all(full_task_graph, parameters, graph_config):
    """Target the tasks which have indicated they should be run on this project
    via the `run_on_projects` attributes."""

    def filter(task):
        return (
            filter_for_tasks_for(task, parameters)
            and task.attributes.get("run-on-pr-type", "all") in ("full-ci", "all")
            and task.attributes.get("release-type") != "release-only"
        )

    return [l for l, task in full_task_graph.tasks.items() if filter(task)]


@register_target_task("pr-normal")
def target_tasks_default(full_task_graph, parameters, graph_config):
    """Target the tasks which have indicated they should be run on this project
    via the `run_on_projects` attributes."""

    def filter(task):
        return (
            filter_for_tasks_for(task, parameters)
            and task.attributes.get("run-on-pr-type", "all") in ("normal-ci", "all")
            and task.attributes.get("release-type") != "release-only"
        )

    return [l for l, task in full_task_graph.tasks.items() if filter(task)]


def filter_release_promotion(full_task_graph, filtered_for_candidates, shipping_phase):
    def filter(task):
        # Include promotion tasks; these will be optimized out
        if task.label in filtered_for_candidates:
            return True

        if task.attributes.get("shipping_phase") == shipping_phase:
            return True

    return [label for label, task in full_task_graph.tasks.items() if filter(task)]


@register_target_task("promote")
def target_tasks_promote(full_task_graph, parameters, graph_config):
    return filter_release_promotion(
        full_task_graph,
        filtered_for_candidates=[],
        shipping_phase="promote",
    )


@register_target_task("ship")
def target_tasks_ship(full_task_graph, parameters, graph_config):
    filtered_for_candidates = target_tasks_promote(
        full_task_graph,
        parameters,
        graph_config,
    )
    return filter_release_promotion(
        full_task_graph, filtered_for_candidates, shipping_phase="ship"
    )
