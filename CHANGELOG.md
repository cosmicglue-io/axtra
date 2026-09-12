# Changelog

## 0.4.0

### Breaking Changes

- Replaced the Cmdline notifier and `notify-error-cmdline` feature with the
  PostHog notifier and `notify-error-posthog`.
- Aligned `axtra` and `axtra_macros` package versions.

### New Features

- Added PostHog `$exception` capture through the official Rust SDK.
- Added notifier shutdown draining so buffered capture providers can flush.
- PostHog exceptions include app, environment, release, severity, stable
  fingerprint, and source location without request URLs or user emails.
- Error fallback pages now encode dynamic text, and expired bouncer entries are
  removed without blocking requests.
- Nested validation paths and generic `ResponseKey` derives are preserved.
- Notification concurrency, request latency, and shutdown latency are bounded.
- CI now verifies formatting, strict Clippy, all features, isolated features,
  package contents, and the Rust 1.88 minimum supported version.
- GitHub Releases now publish both crates with short-lived crates.io Trusted
  Publishing credentials.

## 0.3.0

### Breaking Changes

- **Slack/Discord providers now use config structs instead of plain URLs**
  - `with_slack(url)` → `with_slack(SlackConfig::new(url))`
  - `with_discord(url)` → `with_discord(DiscordConfig::new(url))`

### New Features

- **Configurable mentions for Slack and Discord providers**
  - `SlackConfig::new(url).with_mention("@oncall")`
  - `DiscordConfig::new(url).with_mention("<@&role_id>")`
  - New env vars: `SLACK_ERROR_MENTION`, `DISCORD_ERROR_MENTION`

### Improvements

- Error page HTML files are now cached after first load (no repeated file I/O)
- Removed hardcoded `@oncall` mention from Slack/Discord providers
- Removed unused `NotifyError::is_transient()` method
- Fixed documentation examples to use correct `NotifyFuture` trait signature

## 0.2.4

- Makes formatted_message public so app_errors can be manually logged 

## 0.2.3

- Adds trust_proxy to bouncer to put behind a trusted proxy for IP resolution

## 0.2.2

- Fixes SPA possible caching of index.html
- Bouncer fixes for IP resolution

## 0.2.1

- Added Bouncer module
- Removed errant info in static file serving

## 0.2.0

- Introduced `app_error!` macros for ergonomic error construction.
- Support for error modifiers: `json`, `html`, `with_error`, and format args.
- Closure-based error mapping for `.map_err()` and underlying error propagation.
- Improved error location tracking and logging.
- Expanded macro documentation and examples.

## 0.1.0

- Initial launch.
- Unified `AppError` enum for Axum APIs.
- TypeScript type generation for error codes and responses.
- `WrappedJson<T>` and `ResponseKey` derive macro for predictable API responses.
- Health check endpoint for Postgres.
- Static file and SPA serving helpers.
- Optional Slack/Discord/Sentry error notification integration.
