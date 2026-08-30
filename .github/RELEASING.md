# Publishing a release

The `Publish crates.io release` workflow publishes every workspace crate in
dependency order. It skips crate versions that already exist, so a failed run
can be restarted safely.

Before publishing:

1. Give every workspace package the same new version and update `Cargo.lock`.
2. Ensure CI passes on `main`.
3. Create a GitHub environment named `crates-io`.
4. Add a `CARGO_REGISTRY_TOKEN` secret to that environment. Use a crates.io API
   token authorized to publish all `bevy-aqua-*` crates.
5. Publish a GitHub release tagged `v<workspace-version>`.

The workflow can also be started manually. Enter the exact workspace version
when prompted. Protect the `crates-io` environment with required reviewers to
prevent accidental publication.
