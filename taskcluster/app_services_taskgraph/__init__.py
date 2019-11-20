# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

from importlib import import_module
import os

from six import text_type
from taskgraph.parameters import extend_parameters_schema
from voluptuous import Required, Any


def register(graph_config):
    """
    Import all modules that are siblings of this one, triggering decorators in
    the process.
    """
    _import_modules(["worker_types", "target_tasks"])
    extend_parameters_schema({
        Required("head_tag"): Any(text_type, None),
    })


def _import_modules(modules):
    for module in modules:
        import_module(".{}".format(module), package=__name__)


def get_decision_parameters(graph_config, parameters):
    parameters["head_tag"] = None
    if parameters["tasks_for"] == "github-release":
        head_tag = os.environ.get("MOZILLA_HEAD_TAG")
        if head_tag is None:
            raise ValueError("Cannot run github-release if the environment variable "
                             "'MOZILLA_HEAD_TAG' is not defined")
        parameters["head_tag"] = head_tag.decode("utf-8")
    elif parameters["tasks_for"] == "github-pull-request":
        pr_title = os.environ.get("MOZILLA_PULL_REQUEST_TITLE", "")
        if "[ci full]" in pr_title:
            print("phwoar")
            parameters["target_tasks_method"] = "pr-full"
        elif "[ci skip]" in pr_title:
            parameters["target_tasks_method"] = "pr-skip"
        else:
            parameters["target_tasks_method"] = "pr-normal"
