import json


# 1. create request
# 2. enroll, convert response to dict from JSON
# 3. map enrolledFeatureConfigMap values to the feature values
# 4. create FmlClient
# 5. merge feature configs into default JSON and validate their values against the manifest
# 6. load JSON response as dict
def test_enroll_and_get_enrolled_feature_json_control(fml_client, cirrus_client):
    req = json.dumps(
        {
            "clientId": "jeddai",
            "requestContext": {"username": "jeddai"},
        }
    )
    res = json.loads(cirrus_client.handle_enrollment(req))
    feature_configs = json.dumps(
        [value["feature"] for value in res["enrolledFeatureConfigMap"].values()]
    )

    assert (
        res["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"]["slug"]
        == "experiment-slug"
    )
    assert (
        res["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
            "branch"
        ]
        == "control"
    )

    fml_client = fml_client("test-include-import.fml.yml", "developer")
    merged_res = fml_client.validate_feature_configs_and_merge_into_defaults(
        feature_configs
    )
    merged_res_json = json.loads(merged_res.json)

    assert merged_res_json["imported-module-1-included-feature-1"]["enabled"] is False
    assert len(merged_res.errors) == 0


# repeat the above but with a different client/username on the request
def test_enroll_and_get_enrolled_feature_json_treatment(fml_client, cirrus_client):
    req = json.dumps(
        {
            "clientId": "test",
            "requestContext": {"username": "test"},
        }
    )
    res = json.loads(cirrus_client.handle_enrollment(req))
    feature_configs = json.dumps(
        [value["feature"] for value in res["enrolledFeatureConfigMap"].values()]
    )

    assert (
        res["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"]["slug"]
        == "experiment-slug"
    )
    assert (
        res["enrolledFeatureConfigMap"]["imported-module-1-included-feature-1"][
            "branch"
        ]
        == "treatment"
    )

    fml_client = fml_client("test-include-import.fml.yml", "developer")
    merged_res = fml_client.validate_feature_configs_and_merge_into_defaults(
        feature_configs
    )
    merged_res_json = json.loads(merged_res.json)

    assert merged_res_json["imported-module-1-included-feature-1"]["enabled"] is True
    assert len(merged_res.errors) == 0
