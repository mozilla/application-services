# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import absolute_import, print_function, unicode_literals

from taskgraph.transforms.base import TransformSequence


transforms = TransformSequence()


@transforms.add
def build_task(config, tasks):
    for task in tasks:
        script = task["worker"].pop("script")
        bash_command = [
            "/bin/bash",
            "--login",
            "-c",
            "cat <<'SCRIPT' > ../script.sh && bash -e ../script.sh\n{}\nSCRIPT".format(script)
        ]

        task["run"]["command"] = bash_command

        yield task

