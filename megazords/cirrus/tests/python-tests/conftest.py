# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
import json

import pytest
from cirrus import CirrusClient, EnrollmentStatusExtraDef, MetricsHandler
from fml import FmlClient


class TestMetricsHandler(MetricsHandler):
    recordings = []

    def record_enrollment_statuses(
        self, enrollment_status_extras: [EnrollmentStatusExtraDef]
    ):
        self.recordings.clear()
        self.recordings.extend(enrollment_status_extras)


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
    test_metrics = TestMetricsHandler()
    client = CirrusClient(app_context, test_metrics, [])
    data = json.dumps({"data": [experiment]})
    client.set_experiments(data)
    return client


@pytest.fixture
def cirrus_client(app_context, bucket_config):
    branches = [
        {
            "slug": "control",
            "ratio": 1,
            "feature": {
                "featureId": "imported-module-1-included-feature-1",
                "value": {"enabled": False},
            },
        },
        {
            "slug": "treatment",
            "ratio": 1,
            "feature": {
                "featureId": "imported-module-1-included-feature-1",
                "value": {"enabled": True},
            },
        },
    ]

    experiment = {
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
        "featureIds": ["imported-module-1-included-feature-1"],
    }

    test_metrics = TestMetricsHandler()
    client = CirrusClient(app_context, test_metrics, [])
    data = json.dumps({"data": [experiment]})
    client.set_experiments(data)
    return client


@pytest.fixture
def fml_client():
    def _client(path, channel):
        return FmlClient(
            "./megazords/cirrus/tests/python-tests/resources/" + path, channel
        )

    return _client
