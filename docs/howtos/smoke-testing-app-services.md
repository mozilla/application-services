# Smoke testing Application Services against end-user apps

This is a great way of finding integration bugs with application services.
It can be done manually using substitution scripts, but we also have scripts that will do all of these for you.

## Firefox iOS

The `automation/smoke-test-firefox-ios.py` script will clone (or use a local version) of Firefox iOS and
run tests against the current application-services worktree.  
Add the `-h` argument to discover all the script exciting options!
