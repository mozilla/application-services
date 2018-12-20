---
id: working-with-reference-browser
title: Development with the Reference Browser
sidebar_label: Development with the Reference Browser
---

## Working on unreleased application services code in android components or reference browser

This is a companion to the [equivalent instructions for the android-components repository](https://mozilla-mobile.github.io/android-components/contributing/testing-components-inside-app).

Modern Gradle supports [composite builds](https://docs.gradle.org/current/userguide/composite_builds.html), which allows to substitute on-disk projects for binary publications.  Composite builds transparently accomplish what is usually a frustrating loop of:
1. change library
1. publishing library snapshot to the local Maven repository
1. consume library snapshot in application

## Preparation

Let's assume that the (two or) three projects are siblings, like:
```sh
git clone https://github.com/mozilla/application-services
git clone https://github.com/mozilla-mobile/android-components
git clone https://github.com/mozilla-mobile/reference-browser
```

## Substituting projects

In `android-components/settings.gradle`:
```groovy
includeBuild('../application-services') {
    dependencySubstitution {
        // As required.
        substitute module('org.mozilla.fxaclient:fxaclient') with project(':fxa-client-library')
        substitute module('org.mozilla.sync15:logins') with project(':logins-library')
        substitute module('org.mozilla.places:places') with project(':places-library')
    }
}
```

In `reference-browser/settings.gradle`:
```groovy
includeBuild('../android-components') {
    dependencySubstitution {
        // As required.
        substitute module('org.mozilla.components:browser-storage-sync') with project(':browser-storage-sync')
        substitute module('org.mozilla.components:service-firefox-accounts') with project(':service-firefox-accounts')
        substitute module('org.mozilla.components:service-sync-logins) with project(':service-sync-logins)
    }
}

// Gradle handles transitive dependencies just fine, but Android Studio doesn't seem to always do
// the right thing.  Duplicate the transitive dependencies from `android-components/settings.gradle`
// here as well.
includeBuild('../application-services') {
    dependencySubstitution {
        // As required.
        substitute module('org.mozilla.fxaclient:fxaclient') with project(':fxa-client-library')
        substitute module('org.mozilla.sync15:logins') with project(':logins-library')
        substitute module('org.mozilla.places:places') with project(':places-library')
    }
}
```

## Caveat

There's a big gotcha with library substitutions: the Gradle build computes lazily, and AARs don't include their transitive dependencies' JNI libraries.  This means that in `android-components`, `./gradlew :service-sync-logins:assembleDebug` **does not** invoke `:logins-library:cargoBuild`, even though `:service-sync-logins` depends on the substitution for `:logins-library` and even if the inputs to Cargo have changed!  It's the final consumer of the `:service-sync-logins` project (or publication) that will incorporate the JNI libraries.

In practice that means _you should always be targeting something that produces an APK_: a test, a sample module, or the Reference Browser itself.  Then you should find that the `cargoBuild` tasks are invoked as you expect.

## Notes

1. Transitive substitutions (as shown above) work but require newer Gradle versions (4.10+).
1. Android Studio happily imports substitutions, but it doesn't appear to always do the right thing with transitive substitutions.  Best to keep substitutions in the final project (i.e., in `reference-browser/settings.gradle`) or to duplicate them in all transitive links.
1. Be aware that the project list can get very large!  At this time, there's no way to filter the project list.
