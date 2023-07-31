#!/bin/bash
dir=$(dirname "$0")
# shellcheck disable=SC1091
source "${dir}/../common.sh"

clear
display_comment "Reinstalling the app"
app_reinstall

display_comment "Open the app to show what normal onboarding looks like"
display_subcomment "This is all shown before, but establishes what normal and success looks like"
waitrun "nimbus-cli --app firefox_ios --channel developer open"

anykey
display_comment "Now we reset the app, and relaunch with an onboarding experiment"
waitrun "nimbus-cli --app firefox_ios --channel developer enroll release-ios-on-boarding-challenge-the-default-copy --branch treatment-a --reset-app"
anykey

clear
display_comment "Reinstalling the app"
app_reinstall

clear
display_subcomment "Note to presenter: Start the server in the other tab— run ./01-run-server-in-a-different-process.sh"
anykey

clear
display_comment "Added to every command that opens the app is a --pbcopy option"
waitrun "nimbus-cli --app firefox_ios --channel developer enroll release-ios-on-boarding-challenge-the-default-copy --branch treatment-a --pbcopy"

display_break
display_comment "pbcopy and pbpaste are the macOS commands to copy and paste to the pastebuffer (the clipboard)"
waitrun "pbpaste"
echo
anykey

clear
display_comment "--pbcopy allows us to combine opening with deeplinks (like the Glean Debug view) and experiments"
waitrun "nimbus-cli --app firefox_ios --channel developer unenroll --deeplink glean?logPings=true --pbcopy"

display_break
display_comment "This shorter command shows how the URL is constructed, with the deeplink and the encoded experiments concatenated"
waitrun "pbpaste"
echo
anykey

clear
display_comment 'We can paste this URL into any app that can get it to the device (e.g. Skype or Slack)'
anykey
display_comment "Or… a webpage"

display_subcomment "(On a device this is a manual step, but we automate this here for demo purposes)"
runwait "xcrun simctl openurl booted http://192.168.1.199:8080"

display_break
display_comment "Running the same command as before, but using pbpaste instead of pbcopy"
waitrun "nimbus-cli --app firefox_ios --channel developer enroll release-ios-on-boarding-challenge-the-default-copy --branch treatment-a --pbpaste"

anykey
clear
display_comment "Let's try the other treatment branch"
app_terminate
display_subcomment "Note to presenter: We need to get Safari back to foreground"
anykey
launch_safari
waitrun "nimbus-cli --app firefox_ios --channel developer enroll release-ios-on-boarding-challenge-the-default-copy --branch treatment-c --pbpaste"

display_break
anykey

clear
display_comment "And now let's try on Android."
display_subcomment "Note to presenter: show Chrome on an Android"
waitrun "nimbus-cli --app fenix --channel developer enroll on-boarding-challenge-the-default --branch treatment-a --pbpaste"

display_comment "Any Questions"
anykey
clear
