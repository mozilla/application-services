import json
import pytest
from cirrus import CirrusClient
from fml import FmlClient


@pytest.fixture
def bucket_config():
    return {
        "randomizationUnit": "user_id",
        "count": 100,
        "namespace": "",
        "start": 1,
        "total": 100,
    }


@pytest.fixture
def branches():
    return [
        {
            "slug": "control",
            "ratio": 1,
            "feature": {"featureId": "feature1", "value": {"value key": "value"}},
        },
        {
            "slug": "treatment",
            "ratio": 1,
            "feature": {
                "featureId": "feature1",
                "value": {"value key": "treatment value"},
            },
        },
    ]


@pytest.fixture
def experiment(bucket_config, branches):
    return {
        "schemaVersion": "1.0.0",
        "slug": "experiment-slug",
        "userFacingName": "",
        "userFacingDescription": "",
        "appId": "test app id",
        "appName": "test app name",
        "channel": "dev",
        # Use is_already_enrolled for sticky targeting, otherwise we check lang, region, and a custom attribute
        "targeting": '(is_already_enrolled) || (username in ["test", "jeddai"])',
        "bucketConfig": bucket_config,
        "isRollout": False,
        "isEnrollmentPaused": False,
        "proposedEnrollment": 10,
        "branches": branches,
        "featureIds": ["feature1"],
    }


@pytest.fixture
def app_context():
    return json.dumps(
        {
            "app_id": "test app id",
            "app_name": "test app name",
            "channel": "dev",
        }
    )


@pytest.fixture
def request_context():
    return {
        "username": "jeddai",
    }


@pytest.fixture
def req(request_context):
    def _req(*, client_id="jeddai", request_context=request_context):
        return json.dumps(
            {
                "clientId": client_id,
                "requestContext": request_context,
            }
        )

    return _req


@pytest.fixture
def client(app_context, experiment):
    c = CirrusClient(app_context)
    data = json.dumps({"data": [experiment]})
    c.set_experiments(data)
    return c


@pytest.fixture(scope="class")
def fml_client(request):
    def _client(_, path, channel):
        return FmlClient("./automation/python-tests/resources/" + path, channel)

    request.cls.fml_client = _client
