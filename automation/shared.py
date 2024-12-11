# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Common code used by the automation python scripts.

import os
import subprocess
import sys
from pathlib import Path


def step_msg(msg):
    print(f"> \033[34m{msg}\033[0m")


def fatal_err(msg):
    print(f"\033[31mError: {msg}\033[0m")
    sys.exit(1)


def run_cmd_checked(*args, **kwargs):
    """Run a command, throwing an exception if it exits with non-zero status."""
    kwargs["check"] = True
    return subprocess.run(*args, **kwargs)  # noqa: PLW1510


def check_output(*args, **kwargs):
    """Run a command, throwing an exception if it exits with non-zero status."""
    return subprocess.check_output(*args, **kwargs, encoding="utf8")


def ensure_working_tree_clean():
    """Error out if there are un-committed or staged files in the working tree."""
    if run_cmd_checked(["git", "status", "--porcelain"], capture_output=True).stdout:
        fatal_err("The working tree has un-committed or staged files.")


def find_app_services_root():
    """Find the absolute path of the Application services repository root."""
    cur_dir = Path(__file__).parent
    while not Path(cur_dir, "LICENSE").exists():
        cur_dir = cur_dir.parent
    return cur_dir.absolute()


def get_moz_remote():
    """
    Get the name of the remote for the official mozilla application-services repo
    """
    for line in check_output(["git", "remote", "-v"]).splitlines():
        split = line.split()
        if (
            len(split) == 3
            and split[1] == "git@github.com:mozilla/application-services.git"
            and split[2] == "(push)"
        ):
            return split[0]
    fatal_err(
        "Can't find remote origin for git@github.com:mozilla/application-services.git"
    )


def set_gradle_substitution_path(project_dir, name, value):
    """Set a substitution path property in a gradle `local.properties` file.

    Given the path to a gradle project directory, this helper will set the named
    property to the given path value in that directory's `local.properties` file.
    If the named property already exists with the correct value then it will
    silently succeed; if the named property already exists with a different value
    then it will noisily fail.
    """
    project_dir = Path(project_dir).resolve()
    properties_file = project_dir / "local.properties"
    step_msg(f"Configuring local publication in project at {properties_file}")
    name_eq = name + "="
    abs_value = Path(value).resolve()
    # Check if the named property already exists.
    if properties_file.exists():
        with properties_file.open() as f:
            for ln in f:
                # Not exactly a thorough parser, but should be good enough...
                if ln.startswith(name_eq):
                    cur_value = ln[len(name_eq) :].strip()
                    if Path(project_dir, cur_value).resolve() != abs_value:
                        fatal_err(
                            f"Conflicting property {name}={cur_value} (not {abs_value})"
                        )
                    return
    # The file does not contain the required property, append it.
    # Note that the project probably expects a path relative to the project root.
    ancestor = Path(os.path.commonpath([project_dir, abs_value]))
    relpath = Path(".")
    for _ in project_dir.parts[len(ancestor.parts) :]:
        relpath /= ".."
    for nm in abs_value.parts[len(ancestor.parts) :]:
        relpath /= nm
    step_msg(f"Setting relative path from {project_dir} to {abs_value} as {relpath}")
    with properties_file.open("a") as f:
        f.write(f"{name}={relpath}\n")


class RefNames:
    """
    Contains the branch and tag names we use for automation.

    Attributes:
        main -- where new development happens
        release_branch -- contains the code for a given major release
        release_pr_branch -- Used for PRs against release_branch for a new version
        start_release_pr_branch -- Used for PRs against main to start a new major release
    """

    def __init__(self, major_version_number, minor_version_number):
        major_version_number = int(major_version_number)
        minor_version_number = int(minor_version_number)
        self.main = "main"
        self.release = f"release-v{major_version_number}"
        self.release_pr = f"cut-v{major_version_number}.{minor_version_number}"
        self.start_release_pr = f"start-release-v{major_version_number+1}"
        self.version_tag = f"v{major_version_number}.{minor_version_number}"
        if minor_version_number == 0:
            self.previous_version_tag = f"v{major_version_number-1}.0"
        else:
            self.previous_version_tag = (
                f"v{major_version_number}.{minor_version_number-1}"
            )
