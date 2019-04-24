### Pull Request checklist ###
<!-- Before submitting the PR, please address each item -->
- [ ] **Quality**: This PR builds and tests run cleanly
  - `cargo test --all` produces no test failures
  - `cargo clippy --all --all-targets --all-features` runs without emitting any warnings
  - `cargo fmt` does not produce any changes to the code
  - `./gradlew ktlint detekt` runs without emitting any warnings
  - `swiftformat --lint --swiftversion 4 megazords && swiftlint` runs without emitting any warnings
  - Note: For changes that need extra cross-platform testing, consider adding `[ci full]` to the PR title.
- [ ] **Tests**: This PR includes thorough tests or an explanation of why it does not
- [ ] **Changelog**: This PR includes a changelog entry or an explanation of why it does not need one
  - Any breaking changes to Swift or Kotlin binding APIs are noted explicitly
- [ ] **Dependencies**: This PR follows our [dependency management guidelines](https://github.com/mozilla/application-services/blob/master/docs/dependency-management.md)
  - Any new dependencies are accompanied by a summary of the due dilligence applied in selecting them.
