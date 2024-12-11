# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

import pytest
from fml import FmlClient


@pytest.fixture
def fml_client():
    def _client(path, channel):
        return FmlClient(
            "./megazords/cirrus/tests/python-tests/resources/" + path, channel
        )

    return _client
