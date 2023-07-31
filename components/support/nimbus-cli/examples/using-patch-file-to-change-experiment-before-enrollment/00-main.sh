#!/bin/bash
dir=$(dirname "$0")
# shellcheck disable=SC1091
source "${dir}/../common.sh"

app=firefox_ios
channel=developer

# app_reinstall
clear
display_comment "The --patch FILE option is added to all commands that accept an experiment"

display_subcomment "Patch files are JSON files of the format given by the defaults command…"

waitrun "nimbus-cli --app $app --channel $channel defaults --output defaults.json"
open_editor "./defaults.json"

anykey
display_subcomment "or the features --multi command"
waitrun "nimbus-cli --app $app --channel $channel features mobile-default-browser-cta-copy-test-ios --branch treatment-a --multi --output feature.json"
open_editor "./feature.json"

anykey
clear
display_comment "We can edit this file, either manually or via other tools"
display_subcomment "Here we replace all the trigger expressions in a message with 'true'…"
runwait "jq '.messaging|{ messaging: { triggers: .triggers|map_values(\"true\") } }' ./defaults.json > true-triggers-patch.json"
open_editor "./true-triggers-patch.json"

display_break
display_subcomment "… or replace the trigger for all messages to ALWAYS"
runwait "jq '.messaging|{ messaging: { messages: .messages|map_values({ trigger: [\"ALWAYS\"]}) } }' feature.json > patch.json"
justrun "code ./patch.json"

clear
display_comment "Once we have the patch files we can use it to patch an experiment before enrolling"
waitrun "nimbus-cli --app $app --channel $channel enroll  mobile-default-browser-cta-copy-test-ios --branch treatment-a --patch patch.json"

anykey
display_comment "To really prove it working, let's switch edit the message"
runwait "jq '.messaging|{ messaging: { messages: .messages|map_values({ title: \"A title from a PATCH file\", trigger: [\"ALWAYS\"]}) } }' feature.json > patch.json"
waitrun "nimbus-cli --app $app --channel $channel enroll  mobile-default-browser-cta-copy-test-ios --branch treatment-a --patch patch.json"

anykey
clear
display_comment "Any Questions?"
anykey
justrun "rm ./*.json"