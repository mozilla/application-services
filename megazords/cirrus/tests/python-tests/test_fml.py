# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
import json

import pytest
from fml import FmlError, InternalError


def test_instantiate_fml_client(fml_client):
    fml_client("test.fml.yml", "developer")


def test_instantiate_fml_client_fails_if_invalid_path(fml_client):
    with pytest.raises(FmlError):
        fml_client("a-random-path", "developer")


def test_instantiate_fml_client_fails_if_invalid_yml(fml_client):
    with pytest.raises(InternalError):
        fml_client("test-invalid.fml.yml", "developer")


def test_instantiate_fml_client_fails_if_invalid_channel(fml_client):
    with pytest.raises(FmlError):
        fml_client("test.fml.yml", "release")


def test_default_json(fml_client):
    client = fml_client("test.fml.yml", "developer")
    defaults = json.loads(client.get_default_json())
    assert defaults["example-feature"]["enabled"] is False
    assert defaults["example-feature"]["something"] == "wicked"

    client = fml_client("test.fml.yml", "nightly")
    defaults = json.loads(client.get_default_json())
    assert defaults["example-feature"]["enabled"] is True
    assert defaults["example-feature"].get("something") is None


def test_merge(fml_client):
    client = fml_client("test.fml.yml", "developer")
    configs = {"example-feature": {"enabled": True}}
    result = client.merge(configs)
    assert len(result.errors) == 0

    result_json = json.loads(result.json)["example-feature"]
    assert result_json["enabled"] is True
    assert result_json["something"] == "wicked"


def test_merge_error_on_invalid_key(fml_client):
    client = fml_client("test.fml.yml", "developer")
    configs = {"example-feature": {"enabled1": False}}
    result = client.merge(configs)

    assert len(result.errors) == 1
    assert isinstance(result.errors[0], FmlError)


def test_merge_error_on_invalid_value(fml_client):
    client = fml_client("test.fml.yml", "developer")
    configs = {"example-feature": {"enabled": "false"}}
    result = client.merge(configs)

    assert len(result.errors) == 1
    assert isinstance(result.errors[0], FmlError)


def test_merge_included_and_imported_features(fml_client):
    client = fml_client(
        "test-include-import.fml.yml",
        "developer",
    )
    configs = {
        "example-feature": {"enabled": True},
        "included-feature-1": {"enabled": True},
        "imported-module-1-included-feature-1": {"enabled": True},
    }
    result = client.merge(configs)

    assert len(result.errors) == 0

    example_feature = json.loads(result.json)["example-feature"]
    assert example_feature["enabled"] is True
    assert example_feature["something"] == "wicked"
    included_feature_1 = json.loads(result.json)["included-feature-1"]
    assert included_feature_1["enabled"] is True
    imported_module_1_feature_1 = json.loads(result.json)["imported-module-1-feature-1"]
    assert imported_module_1_feature_1["enabled"] is True
    imported_module_1_included_feature_1 = json.loads(result.json)[
        "imported-module-1-included-feature-1"
    ]
    assert imported_module_1_included_feature_1["enabled"] is True


def test_get_coenrolling_feature_ids(fml_client):
    client = fml_client("test-include-import.fml.yml", "developer")
    result = client.get_coenrolling_feature_ids()

    assert result == ["example-feature", "imported-module-1-included-feature-1"]
