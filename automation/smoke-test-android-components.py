#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Purpose: Run android-components tests against this application-services working tree.
# Dependencies: PyYAML
# Usage: ./automation/smoke-test-android-components.py

import argparse
import re
import sys
import tempfile
from pathlib import Path

import yaml
from shared import (
    fatal_err,
    find_app_services_root,
    run_cmd_checked,
    set_gradle_substitution_path,
    step_msg,
)

parser = argparse.ArgumentParser(
    description="Run android-components tests against this application-services working tree."
)

group = parser.add_mutually_exclusive_group()
group.add_argument(
    "--use-local-repo",
    metavar="LOCAL_REPO_PATH",
    help="Use a local copy of a-c instead of cloning it.",
)
group.add_argument(
    "--remote-repo-url",
    metavar="REMOTE_REPO_URL",
    help="Clone a different a-c repository.",
)
parser.add_argument("--branch", help="Branch of a-c to use.")
parser.add_argument(
    "--action",
    # XXX TODO: it would be very nice to have a "launch sample app" helper here as well.
    choices=["run-tests", "do-nothing"],
    help="Run the following action once a-c is set up.",
)

DEFAULT_REMOTE_REPO_URL = "https://github.com/mozilla-mobile/android-components.git"

args = parser.parse_args()
local_repo_path = args.use_local_repo
remote_repo_url = args.remote_repo_url
branch = args.branch
action = args.action

repo_path = local_repo_path
if repo_path is None:
    repo_path = tempfile.mkdtemp(suffix="-a-c")
    if remote_repo_url is None:
        remote_repo_url = DEFAULT_REMOTE_REPO_URL
    step_msg(f"Cloning {remote_repo_url}")
    run_cmd_checked(["git", "clone", remote_repo_url, repo_path])
    if branch is not None:
        run_cmd_checked(["git", "checkout", branch], cwd=repo_path)
elif branch is not None:
    fatal_err(
        "Cannot specify fenix branch when using a local repo; check it out locally and try again."
    )

step_msg(f"Configuring {repo_path} to autopublish appservices")
set_gradle_substitution_path(
    repo_path, "autoPublish.application-services.dir", find_app_services_root()
)

if action == "do-nothing":
    sys.exit(0)
elif action == "run-tests" or action is None:
    # There are a lot of non-app-services-related components and we don't want to run all their tests.
    # Read the build config to find which projects actually depend on appservices.
    # It's a bit gross but it makes the tests run faster!
    # First, find out what names a-c uses to refer to apservices projects in dependency declarations.
    dep_names = set()
    dep_pattern = re.compile(
        "\\s*const val ([A-Za-z0-9_]+) = .*Versions.mozilla_appservices"
    )
    with Path(
        repo_path, "buildSrc", "src", "main", "java", "Dependencies.kt"
    ).open() as f:
        for ln in f:
            m = dep_pattern.match(ln)
            if m is not None:
                dep_names.add(m.group(1))
    step_msg(f"Found the following appservices dependency names: {dep_names}")
    # Now find all projects that depend on one of those names.
    projects = set()
    with Path(repo_path, ".buildconfig.yml").open() as f:
        buildconfig = yaml.safe_load(f.read())
    for project, details in buildconfig["projects"].items():
        build_dot_gradle = Path(repo_path, details["path"], "build.gradle")
        if not build_dot_gradle.exists():
            build_dot_gradle = Path(repo_path, details["path"], "build.gradle.kts")
            if not build_dot_gradle.exists():
                continue
        with build_dot_gradle.open() as f:
            for ln in f:
                for dep_name in dep_names:
                    if dep_name in ln:
                        projects.add(project)
                        break
    step_msg(
        f"Running android-components tests for {len(projects)} projects: {projects}"
    )
    run_cmd_checked(
        ["./gradlew"] + [f"{project}:test" for project in projects], cwd=repo_path
    )
else:
    print("Sorry I did not understand what you wanted. Good luck!")
