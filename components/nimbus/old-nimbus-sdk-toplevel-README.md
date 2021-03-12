# nimbus-sdk [![CircleCI](https://circleci.com/gh/mozilla/nimbus-sdk/tree/main.svg?style=svg)](https://circleci.com/gh/mozilla/nimbus-sdk/tree/main)
Cross Platform Rapid Experiments "Nimbus" SDK

## Changelog

New and significant features should be listed in the [CHANGELOG.md](./CHANGELOG.md) in the section `Unreleased Changes`.

Before issuing a new release, the `Unreleased Changes` section should be renamed to the version that is being released and match with the tagged version, and a new `Unreleased Changes` section added to the top of the document.

## Cutting a release

We use [cargo-release](https://crates.io/crates/cargo-release) to simplify the release process.
Steps:

1. Ensure your local `main` branch is up to date - the process might try and push this to update tags.
   * `git checkout main`
   * `git pull`
2. If this is a major or minor release, start a new branch for the series (note the `x` below is a literal 'x'):
    * `git checkout -b release-vX.Y.x`
    * `git push -u origin release-vX.Y.x`
   Otherwise, switch to the existing branch.
3. Update `CHANGELOG.md` as noted above, and commit your changes.
4. Switch to the 'nimbus' directory (`cargo release` doesn't work in the root of the repo)
5. Run `cargo release --dry-run -vv [major|minor|patch]` and check that the things
   it is proposing to do seem reasonable. (note that this requires [cargo-release](https://lib.rs/crates/cargo-release))
6. Run `cargo release [major|minor|patch]` to publish the release to github.
7. Make a PR from your branch to request it be merged to the main branch.
8. Check the tag was pushed correctly in the github UI - if not, try `git push --tags`

## Useful Resources

* **[Issue Tracker / Epic](https://jira.mozilla.com/browse/SYNC-1528)**
* **[Project Plan Page](https://mana.mozilla.org/wiki/pages/viewpage.action?pageId=126619091)**
* [Bucketing Technical Documentation](https://docs.google.com/document/d/1WAForAUIchVPaiZFCJO3hNQHY_7KZAjddfscTM_Lx0Y/edit#)
* [Nimbus Mana Page](https://mana.mozilla.org/wiki/display/FJT/Project+Nimbus)
* [mozilla/nimbus-shared - Data and Schemas used across Project Nimbus](https://github.com/mozilla/nimbus-shared)
* [mozilla/uniffi-rs](https://github.com/mozilla/uniffi-rs)
* [mozilla/jexl-rs](https://github.com/mozilla/jexl-rs)

## 

<img src=https://app.lucidchart.com/publicSegments/view/59a408c7-3a09-422c-8eb2-950a7d81cdb9/image.jpeg width=600 />
