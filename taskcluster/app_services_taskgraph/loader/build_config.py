# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# XXX: This loader generates a new build task for every component defined in
# `.buildconfig-android.yml`


from taskgraph.loader.transform import loader as base_loader

from ..build_config import get_components


def loader(kind, path, config, params, loaded_tasks):
    config["tasks"] = {
        component["name"]: {"attributes": {"buildconfig": component}}
        for component in get_components()
    }

    return base_loader(kind, path, config, params, loaded_tasks)
