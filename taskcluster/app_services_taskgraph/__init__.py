# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from importlib import import_module
from voluptuous import Optional
import os

from taskgraph.parameters import extend_parameters_schema
from . import branch_builds
from .build_config import get_version

def register(graph_config):
    # Import modules to register decorated functions
    _import_modules([
        "branch_builds",
        "job",
        "target_tasks",
        "worker_types"
    ])

    extend_parameters_schema({
        Optional('branch-build'): {
            Optional('android-components-branch'): str,
            Optional('fenix-branch'): str,
        },
    })

def _import_modules(modules):
    for module in modules:
        import_module(f".{module}", package=__name__)


def get_decision_parameters(graph_config, parameters):
    if parameters["tasks_for"] == "github-release":
        head_tag = parameters["head_tag"]
        if not head_tag:
            raise ValueError(
                "Cannot run github-release if `head_tag` is not defined. Got {}".format(
                    head_tag
                )
            )
        version = get_version()
        # XXX: tags are in the format of `v<semver>`
        if head_tag[1:] != version:
            raise ValueError(
                "Cannot run github-release if tag {} is different than in-tree "
                "{version} from buildconfig.yml".format(head_tag[1:], version)
            )
    elif parameters["tasks_for"] == "github-pull-request":
        pr_title = os.environ.get("APPSERVICES_PULL_REQUEST_TITLE", "")
        if "[ci full]" in pr_title:
            parameters["target_tasks_method"] = "pr-full"
        elif "[ci skip]" in pr_title:
            parameters["target_tasks_method"] = "pr-skip"
        else:
            parameters["target_tasks_method"] = "pr-normal"

    branch_builds.update_decision_parameters(parameters)
