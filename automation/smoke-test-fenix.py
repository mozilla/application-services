#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.


# Purpose: Run Fenix tests against this application-services working tree.
# Usage: ./automation/smoke-test-fenix.py

import argparse
import sys
import tempfile

from shared import (
    fatal_err,
    find_app_services_root,
    run_cmd_checked,
    set_gradle_substitution_path,
    step_msg,
)

parser = argparse.ArgumentParser(
    description="Run Fenix tests against this application-services working tree."
)

group = parser.add_mutually_exclusive_group()
group.add_argument(
    "--use-local-repo",
    metavar="LOCAL_REPO_PATH",
    help="Use a local copy of fenix instead of cloning it.",
)
group.add_argument(
    "--remote-repo-url",
    metavar="REMOTE_REPO_URL",
    help="Clone a different fenix repository.",
)
group = parser.add_mutually_exclusive_group()
group.add_argument(
    "--use-local-ac-repo",
    metavar="LOCAL_AC_REPO_PATH",
    help="Use a local copy of a-c instead of latest release",
)
group.add_argument(
    "--remote-ac-repo-url",
    metavar="REMOTE_AC_REPO_URL",
    help="Use a clone of a-c repo instead of latest release.",
)
parser.add_argument("--branch", help="Branch of fenix to use.")
parser.add_argument(
    "--ac-branch", default="main", help="Branch of android-components to use."
)
parser.add_argument(
    "--action",
    # XXX TODO: it would be very nice to have a "launch the app" helper here as well.
    choices=["run-tests", "do-nothing"],
    help="Run the following action once fenix is set up.",
)

DEFAULT_REMOTE_REPO_URL = "https://github.com/mozilla-mobile/fenix.git"

args = parser.parse_args()
local_repo_path = args.use_local_repo
remote_repo_url = args.remote_repo_url
local_ac_repo_path = args.use_local_ac_repo
remote_ac_repo_url = args.remote_ac_repo_url
fenix_branch = args.branch
ac_branch = args.branch
action = args.action

repo_path = local_repo_path
if repo_path is None:
    repo_path = tempfile.mkdtemp(suffix="-fenix")
    if remote_repo_url is None:
        remote_repo_url = DEFAULT_REMOTE_REPO_URL
    step_msg(f"Cloning {remote_repo_url}")
    run_cmd_checked(["git", "clone", remote_repo_url, repo_path])
    if fenix_branch is not None:
        run_cmd_checked(["git", "checkout", fenix_branch], cwd=repo_path)
elif fenix_branch is not None:
    fatal_err(
        "Cannot specify fenix branch when using a local repo; check it out locally and try again."
    )

ac_repo_path = local_ac_repo_path
if ac_repo_path is None:
    if remote_ac_repo_url is not None:
        ac_repo_path = tempfile.mkdtemp(suffix="-fenix")
        step_msg(f"Cloning {remote_ac_repo_url}")
        run_cmd_checked(["git", "clone", remote_ac_repo_url, ac_repo_path])
        if ac_branch is not None:
            run_cmd_checked(["git", "checkout", ac_branch], cwd=ac_repo_path)
elif ac_branch is not None:
    fatal_err(
        "Cannot specify a-c branch when using a local repo; check it out locally and try again."
    )

step_msg(f"Configuring {repo_path} to autopublish appservices")
set_gradle_substitution_path(
    repo_path, "autoPublish.application-services.dir", find_app_services_root()
)
if ac_repo_path is not None:
    step_msg(
        f"Configuring {repo_path} to autopublish android-components from {ac_repo_path}"
    )
    set_gradle_substitution_path(
        repo_path, "autoPublish.android-components.dir", ac_repo_path
    )

if action == "do-nothing":
    sys.exit(0)
elif action == "run-tests" or action is None:
    # Fenix has unittest targets for a wide variety of different configurations.
    # It's not useful to us to run them all, so just pick the one that sounds like it's
    # least likely to be broken for unrelated reasons.
    step_msg("Running fenix tests")
    run_cmd_checked(["./gradlew", "app:testNightlyUnitTest"], cwd=repo_path)
    # XXX TODO: I would also like to run the sync integration tests described here:
    #   https://docs.google.com/document/d/1dhxlbGQBA6aJi2Xz-CsJZuGJPRReoL7nfm9cYu4HcZI/
    # However they do not currently pass reliably on my machine.
else:
    print("Sorry I did not understand what you wanted. Good luck!")
