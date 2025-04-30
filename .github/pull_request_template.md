### Pull Request checklist ###
<!-- Before submitting the PR, please address each item -->
- **Breaking changes**:  This PR follows our [breaking change policy](https://github.com/mozilla/application-services/blob/main/docs/howtos/breaking-changes.md)
  - [ ] This PR follows the breaking change policy:
     - This PR has no breaking API changes, or
     - There are corresponding PRs for our consumer applications that resolve the breaking changes and have been approved
- [ ] **Quality**: This PR builds and tests run cleanly
  - Note:
    - For changes that need extra cross-platform testing, consider adding `[ci full]` to the PR title.
    - If this pull request includes a breaking change, consider [cutting a new release](https://github.com/mozilla/application-services/blob/main/docs/howtos/releases.md) after merging.
- [ ] **Tests**: This PR includes thorough tests or an explanation of why it does not
- [ ] **Changelog**: This PR includes a changelog entry in [CHANGELOG.md](../CHANGELOG.md) or an explanation of why it does not need one
  - Any breaking changes to Swift or Kotlin binding APIs are noted explicitly
- [ ] **Dependencies**: This PR follows our [dependency management guidelines](https://github.com/mozilla/application-services/blob/main/docs/dependency-management.md)
  - Any new dependencies are accompanied by a summary of the due diligence applied in selecting them.
