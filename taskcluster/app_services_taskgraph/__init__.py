# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.


from importlib import import_module
from voluptuous import Optional
import os

from taskgraph.parameters import extend_parameters_schema
from . import branch_builds
from . import nightly_builds
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
            Optional('firefox-android-owner'): str,
            Optional('firefox-android-branch'): str,
        },
        'nightly-build': bool,
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

    # We don't have a great way of determining if something is a nightly or
    # not.  But for now, we can assume all cron-based builds are nightlies.
    parameters["nightly-build"] = (parameters["tasks_for"] == "cron")
    parameters['branch-build'] = branch_builds.calc_branch_build_param(parameters)
    parameters['filters'].extend([
        'branch-build',
        'nightly-build',
    ])
