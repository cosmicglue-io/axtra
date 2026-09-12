# Releasing Axtra

Axtra publishes from a GitHub Release through crates.io Trusted Publishing.
The workflow uses short-lived OIDC credentials; do not add a crates.io API
token to GitHub secrets.

## One-time crates.io setup

Configure the same trusted publisher for both `axtra_macros` and `axtra`:

- GitHub organization: `cosmicglue-io`
- Repository: `axtra`
- Workflow: `publish.yml`
- Environment: `release`

## Release

1. Merge the version and changelog update to `main` after CI passes.
2. Run `bin/publish` from a clean `main`, or create a GitHub Release whose tag
   is `axtra-v<version>` and targets that commit.
3. Publishing the GitHub Release triggers the workflow, which tests the
   workspace, publishes `axtra_macros`, waits for it to become available, then
   publishes `axtra`.

Rerunning a failed workflow is safe: versions already visible on crates.io are
skipped.
