# Contributing to wagr-contracts

Thanks for helping build Wagr.

1. Pick an open issue, comment that you'd like to work on it, and wait to be assigned before starting.
2. Fork the repo and create a branch: `git checkout -b feat/<short-name>`.
3. Make your change with tests where it makes sense, and make sure these pass locally:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

4. Open a pull request that links the issue (`Closes #123`), explains what changed and how you tested it, and includes screenshots for UI changes.
5. Keep discussion on the issue or PR. If you get stuck or can't continue, say so, so the issue can be reassigned.

Please only submit code you understand and have run. AI-assisted code is fine, but untested or unexplained changes will not be merged.

By contributing you agree that your work is licensed under the MIT License.
