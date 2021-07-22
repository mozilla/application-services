#!/bin/sh

set -ex

# Make the reports file, which combines all reports together (currently we just have 1)
touch reports
cat lcov.info >> reports
echo '<<<<<< EOF' >> reports

# Calculate query params (most of this was taken from the bash uploader script)

# https://circleci.com/docs/environment-variables

service="circleci"
branch="$CIRCLE_BRANCH"
build="$CIRCLE_BUILD_NUM"
job="$CIRCLE_NODE_INDEX"
if [ "$CIRCLE_PROJECT_REPONAME" != "" ];
then
  slug="$CIRCLE_PROJECT_USERNAME/$CIRCLE_PROJECT_REPONAME"
else
  # git@github.com:owner/repo.git
  slug="${CIRCLE_REPOSITORY_URL##*:}"
  # owner/repo.git
  slug="${slug%%.git}"
fi
pr="${CIRCLE_PULL_REQUEST##*/}"
commit="$CIRCLE_SHA1"


query="commit=${commit}&branch=${branch}&build=${build}&job=${job}&slug=${slug}&service=${service}&pr=${pr}"
url="https://codecov.io/upload/v2?${query}"

echo "Codecov URL: ${url}"

curl -X POST --data-binary @reports "${url}"
