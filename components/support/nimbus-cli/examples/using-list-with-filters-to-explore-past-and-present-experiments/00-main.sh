#!/bin/bash
dir=$(dirname "$0")
# shellcheck disable=SC1091
source "${dir}/../common.sh"
clear

display_comment "The list command used to be fairly prefunctory"
waitrun "nimbus-cli --app firefox_ios --channel developer list"

display_break
display_comment "You can use the Experiment API instead of Remote Settings"
waitrun "nimbus-cli --app firefox_ios --channel developer list --use-api"

anykey
clear
display_comment "I made the --app and --channel optional"
runwait "nimbus-cli list"

anykey
clear
display_comment "And added filters to the list and fetch-list"
runwait "nimbus-cli --app firefox_desktop list --use-api"

anykey
clear
display_comment "Filters can be on app, channel, feature…"
runwait "nimbus-cli --app firefox_ios list --feature onboarding"

anykey
clear
display_comment "… and is-rollout …"
runwait "nimbus-cli --app firefox_ios list --is-rollout=true --channel release"

anykey
clear
display_comment "… and if the experiment was active on a particular day"
runwait "nimbus-cli --app firefox_ios list --active-on 2023-07-01"

anykey
clear
