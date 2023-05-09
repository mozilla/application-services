from components.nimbus.cirrus import NimbusError
import json


def test_one_experiment_should_enroll(client, req):
    response = json.loads(client.handle_enrollment(req()))

    assert len(response["enrollments"]) == 1
    assert "Enrolled" in response["enrollments"][0]["status"].keys()
    assert len(list(response["enrolledFeatureConfigMap"].values())) == 1
    assert len(response["events"]) == 1


def test_one_experiment_should_not_enroll(client, req, request_context):
    request_context["username"] = "not test"

    request = req(
        request_context=request_context
    )

    response = json.loads(client.handle_enrollment(request))

    assert len(response["enrollments"]) == 1
    assert "NotEnrolled" in response["enrollments"][0]["status"].keys()
    assert len(list(response["enrolledFeatureConfigMap"].values())) == 0
    assert len(response["events"]) == 0


def test_one_experiment_should_enroll_1000_times(client, req):
    response = None
    for i in range(0, 1000):
        response = json.loads(client.handle_enrollment(req()))

    assert len(response["enrollments"]) == 1
    assert "Enrolled" in response["enrollments"][0]["status"].keys()
    assert len(list(response["enrolledFeatureConfigMap"].values())) == 1
    assert len(response["events"]) == 1


def test_failure_case_no_client_id(client, req):
    request = req(client_id = None)

    try:
        client.handle_enrollment(request)
    except NimbusError:
        pass
    else:
        assert False, 'client.handle_enrollment did not throw an error'
