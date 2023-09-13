# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
import json
import pytest
from fml import FmlError, InternalError

def test_smoke_test(fml_client):
    fml_client("test.fml.yml", "developer")
