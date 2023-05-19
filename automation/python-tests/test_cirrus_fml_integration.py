import json
import pytest
import unittest


@pytest.mark.usefixtures("fml_client", "cirrus_client")
class TestCirrusClientFmlClientIntegration(unittest.TestCase):
    def test_enroll_and_get_enrolled_feature_json(self):
        # create request
        req_1 = json.dumps(
            {
                "clientId": "jeddai",
                "requestContext": {"username": "jeddai"},
            }
        )
        # enroll, convert response to dict from JSON
        res_1 = json.loads(self.cirrus_client.handle_enrollment(req_1))
        # map enrolledFeatureConfigMap values to the feature values
        feature_configs = json.dumps(
            [value["feature"] for value in res_1["enrolledFeatureConfigMap"].values()]
        )

        assert (
            res_1["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
                "slug"
            ]
            == "experiment-slug"
        )
        assert (
            res_1["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
                "branch"
            ]
            == "control"
        )

        # create FmlClient
        fml_client = self.fml_client("test-include-import.fml.yml", "developer")
        # merge feature configs into default JSON and validate their values against the manifest
        merged_res_1 = fml_client.validate_feature_configs_and_merge_into_defaults(
            feature_configs
        )
        # load JSON response as dict
        merged_res_json_1 = json.loads(merged_res_1.json)

        assert (
            merged_res_json_1["imported-module-1-included-feature-1"]["enabled"]
            is False
        )
        assert len(merged_res_1.errors) == 0

        # repeat the above but with a different client/username on the request
        req_2 = json.dumps(
            {
                "clientId": "test",
                "requestContext": {"username": "test"},
            }
        )
        res_2 = json.loads(self.cirrus_client.handle_enrollment(req_2))
        feature_configs = json.dumps(
            [value["feature"] for value in res_2["enrolledFeatureConfigMap"].values()]
        )

        assert (
            res_2["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
                "slug"
            ]
            == "experiment-slug"
        )
        assert (
            res_2["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
                "branch"
            ]
            == "treatment"
        )

        fml_client = self.fml_client("test-include-import.fml.yml", "developer")
        merged_res_2 = fml_client.validate_feature_configs_and_merge_into_defaults(
            feature_configs
        )
        merged_res_json_2 = json.loads(merged_res_2.json)

        assert (
            merged_res_json_2["imported-module-1-included-feature-1"]["enabled"] is True
        )
        assert len(merged_res_2.errors) == 0
