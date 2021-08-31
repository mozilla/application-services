# Application Services Release Process

## Make a new release from latest main.
1. Create a release commit.
    - **Automated process:** run the `./automation/prepare-release.py` script to create a release commit and open a pull-request.

        Note that after this script is executed, the following should be true:
        - The library version has been updated in `.buildconfig-android.yml`.
        - The entries in `CHANGES_UNRELEASED.md` have been moved to `CHANGELOG.md` under a header with the new version number and a link to the full changelog since the previous release.
        - The full changelog link in `CHANGES_UNRELEASED.md` has been updated with the new release version.

    - **Manual process:** if the automated process above fails, refer to the [Create a release commit manually](#create-a-release-commit-manually) section.

    **Note:** Because the release commit must be on the main branch, your PR will need to be approved, CI successful, and merged before a release can be cut.
2. Cut the actual release.
    1. Click "Releases", and then "Draft a New Release" in the github UI.
    2. Enter `v<myversion>` as the tag. It's important this is the same as the tags that are in the changelog.
    3. Under the description, paste the contents of the release notes from CHANGELOG.md.
    4. Note that the release is not avaliable until the taskcluster build completes for that tag.
        - Finding this out takes a little navigation in the github UI. It's available at `https://github.com/mozilla/application-services/commits/v<VERSION NUMBER>` in the build status info (the emoji) next to the last commit.
        - If the taskcluster tag and/or release tasks fail, ping someone in slack and we'll figure out what to do.
    5. Click "Publish Release".
3. Inform consumers that the new release is available so that [android-components](https://github.com/mozilla-mobile/android-components) can be updated. If you will be creating the update, do the following:
    1. If the changes expose new functionality, or otherwise require changes to code or documentation in https://github.com/mozilla-mobile/android-components, perform those. This part is often done at the same time as the changes in application-services, to avoid being blocked on steps 3-4 of this document.
    2. Change the versions of our dependencies in [buildSrc/src/main/java/Dependencies.kt](https://github.com/mozilla-mobile/android-components/blob/master/buildSrc/src/main/java/Dependencies.kt).
    3. Note the relevant changes in their [docs/changelog.md](https://github.com/mozilla-mobile/android-components/blob/master/docs/changelog.md), and update the application-services version there as well in their list of dependency versions.
    4. **_Important: Manually test the changes versus the samples in android-components._**
        - We do not have automated test coverage for much of the network functionality at this point, so this is crucial.
        - You can do this using the smoketest instructions below.
            **Note:** iOS smoke tests can only be run on macs.
            - Run the `./automation/smoke-test-firefox-ios.py` script to test integration with Firefox for iOS.
            - Run the `./automation/smoke-test-android-components.py` script to test integration with Android Components.
            - Run the `./automation/smoke-test-fenix.py` script to test integration with Fenix.
    5. Get it PRed and landed.

**Note:** If you need to manually produce the iOS build for some reason (for example, if CircleCI cannot), someone with a mac needs to do the following steps:
    1. If necessary, set up for performing iOS builds using `./libs/verify-ios-environment.sh`.
    2. Run `./build-carthage.sh` in the root of the repository.
    3. Upload the resulting `MozillaAppServices.framework.zip` as an attachment on the github release.

---
## Make a new point-release from an existing release that is behind latest main.

1. If necessary, make a new branch named `release-vXX` which will be used for all point-releases on the `vXX.YY.ZZ`
   series. Example:
    ```
    git checkout -b release-v72 v72.1.0
    git push -u origin release-v72
    ```
2. Make a new branch with any fixes to be included in the release, *remembering not to make any breaking API changes.*. This may involve cherry-picking fixes from main, or developing a new fix directly against the branch. Example:
    ```
    git checkout -b fixes-for-v72.1.1 release-v72
    git cherry-pick 37d35304a4d1d285c8f6f3ce3df3c412fcd2d6c6
    git push -u origin fixes-for-v72.1.1
    ```
3. Get a PR up with your changes and land them into the "base" branch.
   For example, if you are making a `release-v72.1.1` release, all the changes
   you want in that release must already be in the `release-v72` branch before
   following the steps below.
4. Follow the above steps for cutting a new release from main, except that:
    * When running the `./automation/prepare-release.py` script, use the `--base-branch` argument to point it at your release branch, and specify `patch` as the release type. Example:
       ```
       ./automation/prepare-release.py --base-branch=release-v72 patch
       ```
    * When opening a PR to land the commits, target the `release-vXX` branch rather than main.
    * When cutting the new release via github's UI, target the `release-vXX` branch rather than main.
5. Merge the new release back to main.
    * This will typically require a PR and involve resolving merge conflicts in the changelog.
    * This ensures we do not accidentally orphan any fixes that were made directly against the release branch,
      and also helps ensure that every release has an easily-discoverable changelog entry in main.
    * When merging the PR, create a merge commit instead of squashing and merging. You can do this by choosing "Merge Pull Request" in GitHub's UI.
    * GitHub may require you to use admin privileges to merge the PR since the `release-vXX` branch is most likely not up to date with main. If there are no merge conflicts to resolve and CI checks pass, you can check the checkbox once the PR is approved.

---
### Create a release commit manually

1. Update the changelog.
    1. Copy the contents from `CHANGES_UNRELEASED.md` into the top of `CHANGELOG.md`, except for the part that links to this document.
    2. In `CHANGELOG.md`:
        1. Replace `# Unreleased Changes` with `# v<new-version-number> (_<current date>_)`.
        2. Replace `main` in the Full Changelog link (which you pasted in from `CHANGES_UNRELEASED.md`) to be `v<new-version-number>`. E.g. if you are releasing 0.13.2, the link should be
            ```
            [Full Changelog](https://github.com/mozilla/application-services/compare/v0.13.1...v0.13.2)
            ```
            Note that this needs three dots (`...`) between the two tags (two dots is different). Yes, the second tag doesn't exist yet, you'll make it later.
        3. Optionally, go over the commits between the past release and this one and see if anything is worth including.
        4. Make sure the changelog follows the format of the other changelog entries. If you have access, [this document](https://docs.google.com/document/d/1oxdGm7OQcsy78NzXjMQKTbfzn21tl9Nopmvo8NCMWmU) is fairly comprehensive. For a concrete example, at the time of this writing, see the [0.13.0](https://github.com/mozilla/application-services/blob/main/CHANGELOG.md#0130-2019-01-09) release notes.
            - Note that we try to provide PR or issue numbers (and links) for each change. Please add these if they are missing.

    3. In `CHANGES_UNRELEASED.md`:
        1. Delete the list of changes that are now in the changelog.
        2. Update the "Full Changelog" link so that it starts at your new version and continues to main. E.g. for 60.0.6 this would be
            ```
            [Full Changelog](https://github.com/mozilla/application-services/compare/v60.0.6...main)
            ```
            Again, this needs 3 dots.

2. Bump `libraryVersion` in the top-level [.buildconfig-android.yml](https://github.com/mozilla/application-services/blob/main/.buildconfig-android.yml) file. Be sure you're following semver, and if in doubt, ask.
3. Land the commits that perform the steps above. This takes a PR, typically, because of branch protection on main.
