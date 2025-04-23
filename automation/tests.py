#!/usr/bin/env python3
"""
Run application-services tests

This script provides an interface to run our test/linting/formating scripts

The first argument specifies the operating mode.  This is either a particular
type of test (rust-tests, swift-lint, etc.), or it's "changes" which is for devs
to test their changes.


Changes Mode:
  - Runs tests/clippy against the changes in your branch as determined by:
    - `git merge-base` to find the last commit from the main branch
    - `git diff` to find changed files from that commit
  - Prioritizes executing the tests in a reasonable amount of time.
  - Runs linting and formatting (clippy, rustfmt, etc).
  - Lets rustfmt fix any issues it finds.  It's recommended to
    add or commit code before running the tests so that you can inspect any
    changes with git diff.

Other Modes:
    - rust-tests
    - rust-min-version-tests
    - rust-clippy
    - rust-fmt
    - ktlint
    - swiftlint
    - swiftformat
    - nss-bindings
    - gradle
    - ios-tests
    - python-tests
"""

import argparse
import json
import os
import platform
import shlex
import subprocess
import sys
import traceback
from enum import Enum
from pathlib import Path

PROJECT_ROOT = Path(__file__).parent.parent
AUTOMATION_DIR = PROJECT_ROOT / "automation"
COMPONENTS_DIR = PROJECT_ROOT / "components"
GRADLE = PROJECT_ROOT / "gradlew"
# Ensure this is a proper path, so we can execute it without searching $PATH.
GRADLE = GRADLE.resolve()


IGNORE_PATHS = set(
    [
        # let's not run tests just for dependency changes
        "megazords/full/DEPENDENCIES.md",
        "megazords/full/android/dependency-licenses.xml",
        "megazords/ios-rust/DEPENDENCIES.md",
    ]
)


def blue_text(text):
    if not sys.stdout.isatty():
        return text
    return f"\033[96m{text}\033[0m"


def yellow_text(text):
    if not sys.stdout.isatty():
        return text
    return f"\033[93m{text}\033[0m"


def get_output(cmdline, **kwargs):
    output = subprocess.check_output(cmdline, **kwargs).decode("utf8")
    return output


def run_command(cmdline, **kwargs):
    print(yellow_text(" ".join(shlex.quote(str(part)) for part in cmdline)))
    subprocess.check_call(cmdline, **kwargs)


def path_is_relative_to(path, other):
    """
    Implementation of Path.is_relative_to() which was only added in python 3.9
    """
    return str(path.resolve()).startswith(str(other.resolve()))


def parse_args():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("mode", help="Testing mode", metavar="MODE")
    parser.add_argument(
        "--base-branch", dest="base_branch", default="main", help="git base branch"
    )
    return parser.parse_args()


def on_darwin():
    return platform.system() == "Darwin"


def get_default_target():
    return get_output(
        [PROJECT_ROOT / "taskcluster" / "scripts" / "detect-target.sh", "--use-rustc"]
    )


def should_run_rust_tests(package, min_version):
    # There are no tests in examples/ packages, so don't waste time on them.
    if "examples" in package.manifest_path.parts:
        return False
    # Skip the error support trybuild tests when running the min version tests, since we keep the
    # trybuild output files pinned to the latest rust version.
    if min_version and package.name == "error-support-tests":
        return False
    return True


class BranchChanges:
    """
    Tracks which files have been changed in this branch
    """

    def __init__(self, base_branch):
        # Calculate the merge base, this is the last commit from the base
        # branch that's present in this branch.
        self.merge_base = get_output(
            [
                "git",
                "merge-base",
                "HEAD",
                base_branch,
            ]
        ).strip()
        # Use the merge base to calculate which files have changed from the
        # base branch.
        raw_paths = get_output(
            [
                "git",
                "diff",
                "--name-only",
                self.merge_base,
            ]
        ).split("\n")
        raw_paths = [p for p in raw_paths if p not in IGNORE_PATHS]
        self.paths = [PROJECT_ROOT.joinpath(p) for p in raw_paths]

    @staticmethod
    def has_unstanged_changes():
        output = get_output(["git", "status", "--porcelain=v1"])
        return any(line and line[1] == "M" for line in output.split("\n"))


class RustPackage:
    """
    A rust package that we want to test and run clippy on
    """

    def __init__(self, cargo_metadata):
        self.cargo_metadata = cargo_metadata
        self.name = cargo_metadata["name"]
        # Use manifest path to select the package in cargo.  This works better
        # when using the --no-default-features flag
        self.manifest_path = Path(cargo_metadata["manifest_path"])
        self.directory = self.manifest_path.parent

    def has_default_features(self):
        return bool(self.cargo_metadata.get("features").get("default"))

    def has_features(self):
        return bool(self.cargo_metadata.get("features"))

    def has_changes(self, branch_changes):
        return any(path_is_relative_to(p, self.directory) for p in branch_changes.paths)


class RustFeatures(Enum):
    """
    A set of features to for testing/clippy

    Rust tests and clippy can be affected by which features are enabled.
    Therefore we want to use different combinations of features when running
    tests/clippy.
    """

    DEFAULT = "default features"
    ALL = "all features"
    NONE = "no features"

    def label(self):
        return self.value

    def cmdline_args(self):
        if self == RustFeatures.DEFAULT:
            return []
        elif self == RustFeatures.ALL:
            return ["--all-features"]
        elif self == RustFeatures.NONE:
            return ["--no-default-features"]


def calc_rust_items(branch_changes=None, default_features_only=False):
    """
    Calculate which items we want to test and run clippy on

    Args:
        branch_changes: only yield items for rust packages that have changes
        default_features_only: only test with the default features

    Returns: list of (RustPackage, RustFeatures) items
    """
    json_data = json.loads(
        get_output(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--format-version",
                "1",
            ]
        )
    )

    packages = [RustPackage(p) for p in json_data["packages"]]

    if branch_changes:
        packages = [p for p in packages if p.has_changes(branch_changes)]

    for p in packages:
        yield p, RustFeatures.DEFAULT

    if default_features_only:
        return

    for p in packages:
        if p.has_features():
            yield p, RustFeatures.ALL
    for p in packages:
        if p.has_default_features():
            yield p, RustFeatures.NONE


def calc_non_workspace_rust_items(branch_changes=None, default_features_only=False):
    """
    Calculate which items are not in our default workspace, but we might want to
    do certain things with.

    Returns the same as calc_rust_items
    """
    for path in ["testing/sync-test/Cargo.toml"]:
        json_data = json.loads(
            get_output(
                [
                    "cargo",
                    "metadata",
                    "--no-deps",
                    "--format-version",
                    "1",
                    "--manifest-path",
                    path,
                ]
            )
        )

        packages = [RustPackage(p) for p in json_data["packages"]]

        if branch_changes:
            packages = [p for p in packages if p.has_changes(branch_changes)]

        for p in packages:
            yield p, RustFeatures.DEFAULT

        if default_features_only:
            return

        for p in packages:
            if p.has_features():
                yield p, RustFeatures.ALL
        for p in packages:
            if p.has_default_features():
                yield p, RustFeatures.NONE


# Define a couple functions to avoid this clippy issue:
# https://github.com/rust-lang/rust-clippy/issues/4612h
#
# The safest way to avoid the issue is running cargo clean.  For changes mode
# we use the faster method of touching the changed files so only they get
# rebuilt.


def cargo_clean():
    """
    Force cargo to rebuild rust files
    """
    run_command(["cargo", "clean"])


def touch_changed_paths(branch_changes):
    """
    Quick version of force_rebuild() for change mode

    This version just touches any changed paths, which causes them to be
    rebuilt, but leaves the rest of the files alone.
    """
    for path in branch_changes.paths:
        if path.exists():
            path.touch()


def print_rust_environment():
    print(f"platform: {platform.uname()}")
    print("rustc version: {}".format(get_output(["rustc", "--version"]).strip()))
    print("cargo version: {}".format(get_output(["cargo", "--version"]).strip()))
    print("rustfmt version: {}".format(get_output(["rustfmt", "--version"]).strip()))
    print("GCC version: {}".format(get_output(["gcc", "--version"]).split("\n")[0]))
    print()


def calc_rust_env(package, features):
    if features == RustFeatures.ALL:
        # nss-sys's --features handling is broken.  Workaround it by using a
        # custom --cfg.  This shouldn't be this way!
        return {**os.environ, "RUSTFLAGS": "--cfg __appsvc_ci_hack"}
    else:
        return None


def run_rust_test(package, features):
    run_command(
        [
            "cargo",
            "test",
            "--manifest-path",
            package.manifest_path,
        ]
        + features.cmdline_args(),
        env=calc_rust_env(package, features),
    )


def run_nss_bindings_test():
    run_command(
        [
            "cargo",
            "run",
            "-p",
            "systest",
        ]
    )


def run_clippy(package, features):
    run_command(
        [
            "cargo",
            "clippy",
            "--all-targets",
            "--manifest-path",
            package.manifest_path,
        ]
        + features.cmdline_args()
        + ["--", "-D", "warnings"],
        env=calc_rust_env(package, features),
    )


def run_ktlint():
    run_command([GRADLE, "ktlint", "detekt"])


def run_swiftlint():
    if on_darwin():
        run_command(["swiftlint", "--strict"])
    elif not docker_installed():
        print("WARNING: On non-Darwin hosts, docker is required to run swiftlint")
        print("WARNING: skipping swiftlint on non-Darwin host")
    else:
        cwd = os.getcwd()

        run_command(
            [
                "docker",
                "run",
                "-it",
                "--rm",
                "-v",
                f"{cwd}:{cwd}",
                "-w",
                cwd,
                "ghcr.io/realm/swiftlint:latest",
                "swiftlint",
                "--strict",
            ]
        )


def run_gradle_tests():
    run_command([GRADLE, "test"])


def run_ios_tests():
    if on_darwin():
        run_command([AUTOMATION_DIR / "run_ios_tests.sh"])
    else:
        print("WARNING: skipping iOS tests on non-Darwin host")


def run_python_tests():
    target = get_default_target()
    run_command(
        [
            PROJECT_ROOT / "taskcluster/scripts/server-megazord-build.py",
            "cirrus",
            target,
        ]
    )
    run_command(
        [
            PROJECT_ROOT / "taskcluster/scripts/server-megazord-build.py",
            "nimbus-experimenter",
            target,
        ]
    )


def cargo_fmt(package=None, fix_issues=False):
    cmdline = ["cargo", "fmt"]
    if package:
        cmdline.extend(["--manifest-path", package.manifest_path])
    else:
        cmdline.append("--all")
    if not fix_issues:
        cmdline.extend(["--", "--check"])
    run_command(cmdline)


def swift_format():
    swift_format_args = [
        "megazords",
        "--exclude",
        "**/Generated",
        "--exclude",
        "megazords/ios-rust/Sources/MozillaRustComponentsWrapper/Nimbus/Utils",
        "--lint",
        "--swiftversion",
        "5",
    ]
    if on_darwin():
        run_command(["swiftformat", *swift_format_args])
    elif not docker_installed():
        print("WARNING: On non-Darwin hosts, docker is required to run swiftformat")
        print("WARNING: skipping swiftformat on non-Darwin host")
    else:
        cwd = os.getcwd()

        run_command(
            [
                "docker",
                "run",
                "-it",
                "--rm",
                "-v",
                f"{cwd}:{cwd}",
                "-w",
                cwd,
                "ghcr.io/nicklockwood/swiftformat:latest",
                *swift_format_args,
            ]
        )


def check_for_fmt_changes(branch_changes):
    print()
    if branch_changes.has_unstanged_changes():
        print("cargo fmt made changes.  Make sure to check and commit them.")
    else:
        print("All checks passed!")


class Step:
    """
    Represents a single step of the testing process
    """

    def __init__(self, name, func, *args, **kwargs):
        self.name = name
        self.func = func
        self.args = args
        self.kwargs = kwargs

    def run(self):
        print()
        print(blue_text(f"Running {self.name}"))
        try:
            self.func(*self.args, **self.kwargs)
        except subprocess.CalledProcessError:
            exit_with_error(1, f"Error while running {self.name}")
        except Exception:
            exit_with_error(
                2,
                f"Unexpected exception while running {self.name}",
                print_exception=True,
            )


def calc_steps(args):
    """
    Calculate the steps needed to run the tests

    Yields a list of (name, func) items.
    """
    if args.mode == "changes":
        # changes mode is complicated enough that it's split off into its own
        # function
        for step in calc_steps_change_mode(args):
            yield step
    elif args.mode == "rust-tests":
        print_rust_environment()
        yield Step("cargo clean", cargo_clean)
        for package, features in calc_rust_items():
            if should_run_rust_tests(package, False):
                yield Step(
                    f"tests for {package.name} ({features.label()})",
                    run_rust_test,
                    package,
                    features,
                )
    elif args.mode == "rust-min-version-tests":
        print_rust_environment()
        yield Step("cargo clean", cargo_clean)
        for package, features in calc_rust_items():
            if should_run_rust_tests(package, True):
                yield Step(
                    f"tests for {package.name} ({features.label()})",
                    run_rust_test,
                    package,
                    features,
                )
    elif args.mode == "rust-clippy":
        print_rust_environment()
        yield Step("cargo clean", cargo_clean)
        for package, features in calc_rust_items():
            yield Step(
                f"clippy for {package.name} ({features.label()})",
                run_clippy,
                package,
                features,
            )
        # non-workspace items aren't tested, but we do run clippy on them to
        # make sure they don't go stale.
        for package, features in calc_non_workspace_rust_items():
            yield Step(
                f"clippy for {package.name} ({features.label()})",
                run_clippy,
                package,
                features,
            )
    elif args.mode == "rust-fmt":
        print_rust_environment()
        yield Step("cargo fmt", cargo_fmt)
    elif args.mode == "ktlint":
        yield Step("ktlint", run_ktlint)
    elif args.mode == "swiftlint":
        yield Step("swiftlint", run_swiftlint)
    elif args.mode == "swiftformat":
        yield Step("swiftformat", swift_format)
    elif args.mode == "nss-bindings":
        print_rust_environment()
        yield Step("NSS bindings test", run_nss_bindings_test)
    elif args.mode == "gradle":
        yield Step("gradle tests", run_gradle_tests)
    elif args.mode == "ios-tests":
        yield Step("ios tests", run_ios_tests)
    elif args.mode == "python-tests":
        yield Step("python tests", run_python_tests)
    else:
        print(f"Invalid mode: {args.mode}")
        sys.exit(1)


def calc_steps_change_mode(args):
    """
    Calculate the steps needed for change mode
    """
    print_rust_environment()
    branch_changes = BranchChanges(args.base_branch)
    rust_items = list(calc_rust_items(branch_changes, default_features_only=True))
    rust_packages = list(set(package for package, _ in rust_items))

    if not rust_items:
        print("no changes found.")
        return

    if branch_changes.has_unstanged_changes():
        subprocess.run(["git", "status"], check=False)
        print()
        print("WARNING: unstaged changes in your branch:")
        print(
            "Consider git add or git commit to stage them since this "
            "script will run cargo fmt"
        )
        print("Continue (Y/N)?")
        if input().lower() != "y":
            sys.exit(0)

    yield Step("touch changed paths", touch_changed_paths, branch_changes)
    for package, features in rust_items:
        yield Step(
            f"tests for {package.name} ({features.label()})",
            run_rust_test,
            package,
            features,
        )
    for package, features in rust_items:
        yield Step(
            f"clippy for {package.name} ({features.label()})",
            run_clippy,
            package,
            features,
        )
    for package in rust_packages:
        yield Step(f"rustfmt for {package.name}", cargo_fmt, package, fix_issues=True)
    yield Step("Check for changes", check_for_fmt_changes, branch_changes)


def main():
    args = parse_args()
    os.chdir(PROJECT_ROOT)
    for step in calc_steps(args):
        step.run()


def exit_with_error(code, text, print_exception=False):
    print()
    print("-" * 78)
    print()
    print(text)
    if print_exception:
        traceback.print_exc()
    sys.exit(code)


def docker_installed():
    result = subprocess.run(
        ["docker"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )

    return result.returncode == 0


if __name__ == "__main__":
    main()
