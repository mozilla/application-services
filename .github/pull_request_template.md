### Pull Request checklist ###
<!-- Before submitting the PR, please address each item -->
- [ ] **Quality**: This PR builds and tests run cleanly
  - `cargo clean; cargo test --all` runs without emitting any warnings
  - `cargo fmt` does not produce any changes to the code
- [ ] **Tests**: This PR includes thorough tests or an explanation of why it does not
- [ ] **Changelog**: This PR includes a changelog entry or an explanation of why it does not need one
  - Any breaking changes to Swift or Kotlin binding APIs are noted explicitly
