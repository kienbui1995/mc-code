# Release Checklist — magic-code

## Pre-release
- [ ] All tests pass: `cd mc && cargo test --workspace`
- [ ] Zero clippy warnings: `cargo clippy --workspace --all-targets`
- [ ] Format check: `cargo fmt --all -- --check`
- [ ] Version bumped in `mc/Cargo.toml` (`[workspace.package] version`)
- [ ] Version bumped in `mc/Formula/magic-code.rb`
- [ ] `CHANGELOG.md` updated (technical, for devs)
- [ ] `RELEASE_NOTES.md` updated (user-facing, non-technical)
- [ ] README.md features list current
- [ ] Docker builds: `cd mc && docker build --target prod -t magic-code:$(cat Cargo.toml | grep '^version' | head -1 | cut -d'"' -f2) .`

## Release
```bash
VERSION=0.2.0
git add -A
git commit -m "feat: v${VERSION} release"
git tag "v${VERSION}"
git push && git push --tags
```

## Post-release
- [ ] GitHub Release created automatically (via `release.yml` workflow)
- [ ] Download artifacts and verify checksums
- [ ] Update Homebrew formula SHA256 hashes
- [ ] Test install: `curl -fsSL .../install.sh | sh && magic-code --version`
- [ ] Verify TUI shows correct version in status bar
