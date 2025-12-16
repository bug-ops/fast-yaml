# Trusted Publishing Setup Guide

This guide documents the steps to complete the migration to OIDC-based Trusted Publishing for crates.io.

## ✅ Completed (Automated)

The GitHub Actions workflow has been updated:
- Added `id-token: write` and `contents: read` permissions to the `publish-crates` job
- Removed `CARGO_REGISTRY_TOKEN` environment variable from the publish step

## 📋 Manual Steps Required

### 1. Configure Trusted Publishing on crates.io

You need to configure Trusted Publishing for each of the 4 crates. For each crate:

1. Go to crates.io and log in
2. Navigate to each crate's settings page:
   - `fast-yaml-core`: https://crates.io/crates/fast-yaml-core/settings
   - `fast-yaml-ffi`: https://crates.io/crates/fast-yaml-ffi/settings
   - `fast-yaml-linter`: https://crates.io/crates/fast-yaml-linter/settings
   - `fast-yaml-parallel`: https://crates.io/crates/fast-yaml-parallel/settings

3. In the "Trusted Publishers" section, add a new publisher with:
   - **Provider**: GitHub Actions
   - **Repository**: `bug-ops/fast-yaml`
   - **Workflow**: `release.yml`

4. Save the configuration

### 2. Test the Release Process

Before deleting the legacy token, test that OIDC publishing works:

1. Create a test release tag (or use a patch version bump)
2. Push the tag to trigger the release workflow
3. Monitor the `publish-crates` job to ensure it successfully publishes using OIDC
4. Verify all 4 crates are published to crates.io

### 3. Delete Legacy Token (After Verification)

Once you've confirmed that OIDC publishing works correctly:

1. Go to GitHub repository settings: https://github.com/bug-ops/fast-yaml/settings/secrets/actions
2. Find and delete the `CARGO_REGISTRY_TOKEN` secret
3. (Optional) Revoke the token on crates.io if you still have access to it

## Benefits of Trusted Publishing

✅ **No token management**: No need to rotate or store long-lived API tokens  
✅ **Better security**: Short-lived OIDC tokens that expire automatically  
✅ **Improved audit trail**: All publishes are tied to specific GitHub Actions runs  
✅ **Scoped permissions**: Tokens are automatically scoped to the specific workflow

## Troubleshooting

### Publishing fails with authentication error

- Verify that Trusted Publishing is correctly configured on crates.io for all 4 crates
- Ensure the repository and workflow name match exactly: `bug-ops/fast-yaml` and `release.yml`
- Check that the job has the required permissions (`id-token: write` and `contents: read`)

### Cannot configure Trusted Publishing

- Make sure you have owner/admin permissions on the crates
- Ensure you're logged in to crates.io with the correct account

## References

- [crates.io Trusted Publishing documentation](https://crates.io/docs/trusted-publishing)
- [GitHub Actions OIDC documentation](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/about-security-hardening-with-openid-connect)
