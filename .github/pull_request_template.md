### Pull Request checklist ###
<!-- Before submitting the PR, please address each item -->
- [ ] **Quality**: This PR builds and tests run cleanly
  - Note:
    - For changes that need extra cross-platform testing, consider adding `[ci full]` to the PR title.
    - If this pull request includes a breaking change, consider [cutting a new release](https://github.com/mozilla/application-services/blob/main/docs/howtos/cut-a-new-release.md) after merging.
- [ ] **Tests**: This PR includes thorough tests or an explanation of why it does not
- [ ] **Changelog**: This PR includes a changelog entry in [CHANGES_UNRELEASED.md](../CHANGES_UNRELEASED.md) or an explanation of why it does not need one
  - Any breaking changes to Swift or Kotlin binding APIs are noted explicitly
- [ ] **Dependencies**: This PR follows our [dependency management guidelines](https://github.com/mozilla/application-services/blob/main/docs/dependency-management.md)
  - Any new dependencies are accompanied by a summary of the due dilligence applied in selecting them.

[Branch builds](https://github.com/mozilla/application-services/blob/main/docs/howtos/branch-builds.md): add `[ac: android-components-branch-name]` and/or `[fenix: fenix-branch-name]` to the PR title.
