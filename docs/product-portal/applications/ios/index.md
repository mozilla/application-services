---
id: overview
title: Building with application-services components on iOS
sidebar_label: Overview
---

## Using Carthage

On iOS, application services components should be consumed via
[Carthage](https://github.com/Carthage/Carthage).

Find an appropriate [release](https://github.com/mozilla/application-services/releases)
and then add a line like the following to your Cartfile:

```
github "mozilla/application-services" "X.Y.Z"
```

Carthage will make available a `RustAppServices.framework` containing all
available application-services components.  Import the required components
from this framework into your project.

## Building from source

If the pre-build Carthage dependencies do not meet your needs, it is possible
to make your own Megazord build by compiling from source.

Unfortunately we haven't written instructions for that yet; until we do so
please reach in #rust-component on Slack for help.

## Other package managers

We may add support for other package managers in future, please reach out
in #rust-components on Slack to discuss your needs.

