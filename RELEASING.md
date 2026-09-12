# Releasing Axtra

Axtra publishes version changes merged to `main` through crates.io Trusted
Publishing. The workflow uses short-lived OIDC credentials; do not add a
crates.io API token to GitHub secrets.

## One-time crates.io setup

Configure the same trusted publisher for both `axtra_macros` and `axtra`:

- GitHub organization: `cosmicglue-io`
- Repository: `axtra`
- Workflow: `publish.yml`
- Environment: `release`

## Release

1. Update both crate versions and the changelog in a pull request.
2. Merge the pull request after CI passes.
3. After CI passes on `main`, the publish workflow skips versions already on
   crates.io, publishes `axtra_macros` before `axtra`, then creates the matching
   `axtra-v<version>` GitHub Release.

Run `bin/publish` from a clean `main` to retry a partial release. Reruns are
safe because versions already present on crates.io are skipped.
