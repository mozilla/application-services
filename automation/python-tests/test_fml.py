import json
import pytest
import unittest
from fml import FmlError, InternalError


@pytest.mark.usefixtures("fml_client")
class TestFmlClient(unittest.TestCase):
    def test_instantiate_fml_client(self):
        self.fml_client("test.fml.yml", "developer")

    def test_instantiate_fml_client_fails_if_invalid_path(self):
        with self.assertRaises(FmlError):
            self.fml_client("a-random-path", "developer")

    def test_instantiate_fml_client_fails_if_invalid_yml(self):
        with self.assertRaises(InternalError):
            self.fml_client("test-invalid.fml.yml", "developer")

    def test_instantiate_fml_client_fails_if_invalid_channel(self):
        with self.assertRaises(FmlError):
            self.fml_client("test.fml.yml", "release")

    def test_default_json(self):
        client = self.fml_client("test.fml.yml", "developer")
        defaults = json.loads(client.get_default_json())
        assert defaults["example-feature"]["enabled"] is False
        assert defaults["example-feature"]["something"] == "wicked"

        client = self.fml_client("test.fml.yml", "nightly")
        defaults = json.loads(client.get_default_json())
        assert defaults["example-feature"]["enabled"] is True
        assert defaults["example-feature"].get("something") is None

    def test_validate(self):
        client = self.fml_client("test.fml.yml", "developer")
        config = {"featureId": "example-feature", "value": {"enabled": True}}
        assert client.validate_feature_config(json.dumps(config)) is True

    def test_validate_false(self):
        client = self.fml_client("test.fml.yml", "developer")
        config = {"featureId": "example-featurea", "value": {"enabled": True}}

        with self.assertRaises(FmlError):
            client.validate_feature_config(json.dumps(config))

    def test_merge_and_validate(self):
        client = self.fml_client("test.fml.yml", "developer")
        configs = [{"featureId": "example-feature", "value": {"enabled": True}}]
        result = client.validate_feature_configs_and_merge_into_defaults(
            json.dumps(configs)
        )
        assert len(result.errors) == 0

        result_json = json.loads(result.json)["example-feature"]
        assert result_json["enabled"] is True
        assert result_json["something"] == "wicked"

    @pytest.mark.skip(reason="This functionality is hindered by EXP-3503")
    def test_merge_and_validate_error_on_invalid_key(self):
        client = self.fml_client("test.fml.yml", "developer")
        configs = [{"featureId": "example-feature", "value": {"enabled1": False}}]
        result = client.validate_feature_configs_and_merge_into_defaults(
            json.dumps(configs)
        )

        assert len(result.errors) == 1
        assert isinstance(result.errors[0], FmlError)

    def test_merge_and_validate_error_on_invalid_value(self):
        client = self.fml_client("test.fml.yml", "developer")
        configs = [{"featureId": "example-feature", "value": {"enabled": "false"}}]
        result = client.validate_feature_configs_and_merge_into_defaults(
            json.dumps(configs)
        )

        assert len(result.errors) == 1
        assert isinstance(result.errors[0], FmlError)

    def test_merge_and_validate_on_included_and_imported_features(self):
        client = self.fml_client(
            "test-include-import.fml.yml",
            "developer",
        )
        configs = [
            {"featureId": "example-feature", "value": {"enabled": True}},
            {"featureId": "included-feature-1", "value": {"enabled": True}},
        ]
        result = client.validate_feature_configs_and_merge_into_defaults(
            json.dumps(configs)
        )

        assert len(result.errors) == 0

        example_feature = json.loads(result.json)["example-feature"]
        assert example_feature["enabled"] is True
        assert example_feature["something"] == "wicked"
        included_feature_1 = json.loads(result.json)["included-feature-1"]
        assert included_feature_1["enabled"] is True
        imported_module_1_feature_1 = json.loads(result.json)[
            "imported-module-1-feature-1"
        ]
        assert imported_module_1_feature_1["enabled"] is True
        imported_module_1_included_feature_1 = json.loads(result.json)[
            "imported-module-1-included-feature-1"
        ]
        assert imported_module_1_included_feature_1["enabled"] is False
