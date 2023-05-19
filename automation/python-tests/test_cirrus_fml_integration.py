import json
import pytest
import unittest


@pytest.mark.usefixtures("fml_client", "cirrus_client")
class TestCirrusClientFmlClientIntegration(unittest.TestCase):
    def test_enroll_and_get_enrolled_feature_json(self):
        req_1 = json.dumps(
            {
                "clientId": "test",
                "requestContext": {"username": "test"},
            }
        )
        res_1 = json.loads(self.cirrus_client.handle_enrollment(req_1))
        feature_configs = json.dumps(
            [value["feature"] for value in res_1["enrolledFeatureConfigMap"].values()]
        )
        print(res_1)
        fml_client = self.fml_client("test-include-import.fml.yml", "developer")
        merged_res_1 = fml_client.validate_feature_configs_and_merge_into_defaults(
            feature_configs
        )
        merged_res_json_1 = json.loads(merged_res_1.json)

        print(merged_res_json_1)
