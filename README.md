# Axtra

###

🎨 Overview
===========

Opinionated helpers for Axum + Astro projects.

> **Warning:** This library is experimental, opinionated, and subject to breaking changes.
> 🐉 Here be dragons 🐉

Axtra 0.4 requires Rust 1.88 or newer. Postgres support through `sqlx` is part
of the core crate because database errors and the health endpoint share that
contract; notification and bouncer integrations remain opt-in features.

## Features

### Error Handling

- **AppError**
  - `AppError`: One error type for all your Axum APIs.
  - Error variants for authentication, authorization, validation, database, not found, and more.
  - Automatic handling of thrown WithRejection<Json<Struct>> and Validation using [`validator`](https://github.com/Keats/validator)

- **Error Macros**
  - `app_error!` macro: Ergonomic error construction for all variants.
  - Captures error location automatically.
  - Supports underlying error mapping for easy use with `.map_err()`.

- **TypeScript Type Generation**
  - Rust error types (`ErrorCode`, validation errors, etc.) exported to TypeScript via [`ts-rs`](https://github.com/Aleph-Alpha/ts-rs).
  - Ensures your frontend and backend share error contracts.

- **Error Notifications**
  - Pluggable notification system with trait-based architecture
  - Built-in providers: Slack, Discord, ntfy, PostHog
  - Create custom providers by implementing `ErrorNotifier` trait
  - Configure via builder pattern or environment variables
  
### Api Responses

- **Wrapped JSON Responses**
  - `WrappedJson<T>`: Automatically wraps responses with a key derived from the type name.
  - `ResponseKey` derive macro: Customize or auto-generate response keys for your types.
  - List responses are pluralized automatically.

### Axum Helpers

- **Health Check Endpoint**
  - Built-in Axum route for checking Postgres connectivity.
  - Returns status, DB health, and timestamp.

- **Static File Serving**
  - Helpers for serving SPA and static files with Axum.

### Bouncer

- **IP Banning and Malicious Path Filtering**
  - Simple IP banning and malicious path filtering middleware for Axum.

### Notifications
- **Pluggable Notification System**
  - Trait-based architecture for custom providers
  - Built-in: Slack, Discord, ntfy, PostHog
  - Easy to add Sentry, PagerDuty, or any custom provider

---

## ErrorHandling

### AppError

AppError is an enum type containing types for handling BadRequest, NotFound, Authorization, Authentication, Database and Exception errors.

AppErrors will be logged automatically with the following severity

| Error Type        | Severity (Log Level) |
|-------------------|---------------------|
| Authentication    | INFO                |
| Authorization     | INFO                |
| BadRequest        | WARN                |
| NotFound          | WARN                |
| Validation        | WARN                |
| Database          | ERROR               |
| Exception         | ERROR               |

- **INFO**: Expected authentication/authorization failures.
- **WARN**: Client errors, invalid input, not found, validation issues.
- **ERROR**: Server-side failures, database errors, and exceptions; configured providers are notified.

### WithRejection

AppError will automatically handle malformed JSON and render BadRequest errors when using WithRejection.

```rust
pub async fn create(
    auth_session: AuthSession,
    State(pool): State<PgPool>,
    WithRejection(Json(payload), _): WithRejection<Json<NewSubscription>, AppError>,
) -> Result<WrappedJson<CheckoutSession>, AppError>
```

### Validator

When using .validate()? in a handler, AppError will automatically catch and render the ValidationErrors.

```rust
#[derive(Debug, Validate, Deserialize)]
struct SignupData {
    #[validate(email)]
    mail: String,
}

handler() -> Result<impl IntoResponse, AppError> {
  let signup_data = SignupData { mail: "notemail" };
  payload.validate()?;
}
```

```json
{
    "status": "Bad Request",
    "message": "There was a validation error with your request.",
    "error": "Validation error",
    "code": "validation",
    "validationErrors": {
        "errors": [
            {
                "field": "mail",
                "code": "email",
                "message": "mail must be a valid email",
            }
        ]
    }
}
```

### Error Macro Usage

The `app_error!` macro makes error construction ergonomic and consistent.  
It automatically tracks error location and supports both direct and closure-based usage.

If no response type is passed, defaults to HTML.

### Standalone (Direct) Usage

```rust
// Basic error (HTML response)
Err(app_error!(bad_request, "Missing field"));

// JSON error
Err(app_error!(bad_request, json, "Invalid JSON payload"));

// HTML error
Err(app_error!(bad_request, html, "Form error"));

// Not found resource: &str
Err(app_error!(not_found, "User not found"));
Err(app_error!(not_found, json, "User not found"));

// Unauthorized resource: &str, action: &str
Err(app_error!(unauthorized, "users", "delete"));
Err(app_error!(unauthorized, json, "users", "delete"));

// Unauthenticated
Err(app_error!(unauthenticated));
Err(app_error!(unauthenticated, json));

// Validation error
Err(app_error!(validation, errors));
Err(app_error!(validation, json, errors));

// Thrown exceptons
Err(app_error!(throw, "You broke something"))
Err(app_error!(throw, json, "You broke something and we're responding with json"))
Err(app_error!(throw, html, "You broke something and we're responding with html"))
```

### Closure (For `.map_err()` and error mapping)

```rust
// Bad request with underlying error
let value: i32 = input.parse().map_err(app_error!(bad_request, with_error, "Invalid number"))?;

// With format args
let value: i32 = input.parse().map_err(app_error!(bad_request, with_error, "Invalid number: {}", input))?;

// JSON error with underlying error
let user: User = serde_json::from_str(&body).map_err(app_error!(bad_request, json, with_error, "Bad JSON: {}", body))?;

// Database error mapping
let user = sqlx::query!("SELECT * FROM users WHERE id = $1", id)
    .fetch_one(&pool)
    .await
    .map_err(app_error!(db, "Failed to fetch user"))?;

// Exception mapping
let result = do_something().map_err(app_error!(exception, "Unexpected error"))?;
```

### Typescript types

Axtra provides Ts-Rs bindings to output typed ErrorResponses.

To enable, add the export to your build.rs

```rust
use axtra::errors::ErrorResponse;
use std::fs;
use std::path::Path;
use ts_rs::TS;

fn main() {
    // Specify the path to the directory containing the TypeScript files
    let ts_dir = Path::new("types");
    fs::create_dir_all(ts_dir).unwrap();

    ErrorResponse::export_all_to(ts_dir).unwrap();
}
```

```typescript
/**
 * Enum of all possible error codes.
 */
export type ErrorCode = "authentication" | "authorization" | "badRequest" | "database" | "exception" | "notFound" | "validation";

export type ErrorResponse = { status: string, message: string, code: ErrorCode, validationErrors?: SerializableValidationErrors, };

/**
 * Represents all validation errors in a serializable form.
 */
export type SerializableValidationErrors = { errors: Array<ValidationFieldError>, };

/**
 * Represents a single field validation error.
 */
export type ValidationFieldError = { field: string, code: string, message: string, params: { [key in string]?: string }, };
```

### Error Notification System

Axtra uses a pluggable, trait-based notification system for sending critical errors to external services.
Providers are opt-in, so applications include only the notification integrations they select.

#### Quick Start

```rust
use axtra::notifier::NotificationManager;
use axtra::errors::notifiers::init_notification_manager;

// Auto-configure from environment variables
init_notification_manager(NotificationManager::from_env());
```

#### Builder Pattern (Recommended)

```rust
use axtra::notifier::{NotificationManager, SlackConfig, DiscordConfig, NtfyConfig, PosthogConfig};
use axtra::errors::notifiers::{init_notification_manager, shutdown_notification_manager};

let manager = NotificationManager::builder()
    .with_slack(SlackConfig::new("https://hooks.slack.com/services/...")
        .with_mention("@oncall"))
    .with_discord(DiscordConfig::new("https://discord.com/api/webhooks/..."))
    .with_ntfy(NtfyConfig::new("my-app-errors"))
    .with_posthog(PosthogConfig::new(std::env::var("POSTHOG_PROJECT_TOKEN").unwrap())
        .with_environment("production")
        .with_service("my-app"))
    .unwrap()
    .build();

init_notification_manager(manager);

// After the server future resolves:
shutdown_notification_manager().await;
```

#### Feature Flags

Enable the providers you need in `Cargo.toml`:

```toml
[dependencies]
axtra = { version = "0.4", features = ["notify-error-slack", "notify-error-posthog"] }
```

| Feature | Provider | Environment Variables |
|---------|----------|----------------------|
| `notify-error-slack` | Slack webhook | `SLACK_ERROR_WEBHOOK_URL`, `SLACK_ERROR_MENTION` (optional) |
| `notify-error-discord` | Discord webhook | `DISCORD_ERROR_WEBHOOK_URL`, `DISCORD_ERROR_MENTION` (optional) |
| `notify-error-ntfy` | [ntfy](https://ntfy.sh) push notifications | `NTFY_TOPIC`, `NTFY_SERVER_URL` (optional), `NTFY_ACCESS_TOKEN` (optional) |
| `notify-error-posthog` | [PostHog](https://posthog.com) error tracking | `POSTHOG_PROJECT_TOKEN`, `POSTHOG_HOST` (optional), `ENVIRONMENT`, `APP_ENV`, or `ENV`, `SERVICE` or `APP_NAME`, `RELEASE` or `VERSION` |

#### Custom Providers

Implement the `ErrorNotifier` trait to add a provider without changing Axtra.

##### Sentry Example

```rust
// In YOUR Cargo.toml (not axtra's):
// sentry = "0.35"  # whatever version you want

use axtra::notifier::{ErrorNotifier, ErrorEvent, NotifyError, NotifyFuture, NotificationManager, SlackConfig};
use reqwest::Client;
use std::sync::Arc;

struct SentryProvider;

impl ErrorNotifier for SentryProvider {
    fn notify<'a>(&'a self, _client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            let level = match event.severity() {
                "critical" => sentry::Level::Fatal,
                "major" => sentry::Level::Error,
                _ => sentry::Level::Warning,
            };

            sentry::with_scope(
                |scope| {
                    scope.set_tag("app_name", &event.app_name);
                    scope.set_tag("location", &event.location);
                },
                || {
                    sentry::capture_message(&event.message, level);
                },
            );
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "sentry"
    }
}

// Usage
let manager = NotificationManager::builder()
    .with_provider(Arc::new(SentryProvider))
    .with_slack(SlackConfig::new("https://hooks.slack.com/..."))
    .build();
```

##### PagerDuty Example

```rust
use axtra::notifier::{ErrorNotifier, ErrorEvent, NotifyError, NotifyFuture};
use reqwest::Client;

struct PagerDutyProvider {
    routing_key: String,
}

impl ErrorNotifier for PagerDutyProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            client.post("https://events.pagerduty.com/v2/enqueue")
                .json(&serde_json::json!({
                    "routing_key": self.routing_key,
                    "event_action": "trigger",
                    "payload": {
                        "summary": event.message,
                        "severity": event.severity(),
                        "source": event.app_name,
                        "custom_details": {
                            "location": event.location,
                            "error_code": format!("{:?}", event.error_code),
                        }
                    }
                }))
                .send()
                .await?;
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "pagerduty"
    }
}
```

---

**Note:**
All notification features are opt-in and only trigger automatically for server-side errors (`Database` and `Exception`).
Notifications are sent concurrently to all configured providers.

---

## Api Responses

### WrappedJson & ResponseKey

Axtra provides a convenient way to wrap API responses with a predictable key, using the `WrappedJson<T>` type and the `ResponseKey` derive macro.

#### Usage

```rust
use axtra::{ResponseKey, response::WrappedJson};
use serde::Serialize;

#[derive(Serialize, ResponseKey)]
struct User {
    id: i32,
    name: String,
}

// In your handler:
async fn get_user() -> Result<WrappedJson<User>, AppError> {
    let user = User { id: 1, name: "Alice".to_string() };
    Ok(WrappedJson(user))
}
```

**Produces JSON:**
```json
{
  "user": {
    "id": 1,
    "name": "Alice"
  }
}
```

#### Customizing the Response Key

You can override the default key by using the `#[response_key = "custom_name"]` attribute:

```rust
#[derive(Serialize, ResponseKey)]
#[response_key = "account"]
struct UserAccount {
    id: i32,
    email: String,
}
```

**Produces JSON:**
```json
{
  "account": {
    "id": 1,
    "email": "alice@example.com"
  }
}
```

#### List Responses

When returning a list, the key is automatically pluralized:

```rust
#[derive(Serialize, ResponseKey)]
struct User {
    id: i32,
    name: String,
}

async fn list_users() -> Result<WrappedJson<Vec<User>>, AppError> {
    let users = vec![
        User { id: 1, name: "Alice".to_string() },
        User { id: 2, name: "Bob".to_string() },
    ];
    Ok(WrappedJson(users))
}
```

**Produces JSON:**
```json
{
  "users": [
    { "id": 1, "name": "Alice" },
    { "id": 2, "name": "Bob" }
  ]
}
```

#### Macro Implementation

```rust
// #[derive(ResponseKey)] will auto-implement this trait:
pub trait ResponseKey {
    fn response_key() -> &'static str;
}
```

See [`axtra_macros::ResponseKey`](./axtra_macros/src/response_key.rs) for details.

---

## Axum Helpers

### Health Check Endpoint

Axtra provides a ready-to-use health check route for monitoring your application's status and database connectivity.

#### Usage

```rust
use axtra::routes::health::check_health;
use axum::{routing::get, Router};
use sqlx::PgPool;

fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/health", get(check_health))
        .with_state(pool)
}
```

**Response (healthy):**
```json
{
  "status": "healthy",
  "postgres": true,
  "timestamp": "2025-07-15T12:34:56Z"
}
```

**Response (degraded):**
- Returns HTTP 503 Service Unavailable if the database is not reachable.

---

### Static File & Single Page App (SPA) Routes

Axtra includes helpers for serving static files and SPAs (such as Astro or React) with Axum.

#### Serve a Single Page App (SPA)

```rust
use axtra::routes::astro::serve_spa;
use axum::Router;

// Serves files from ./dist/myapp/index.html for /myapp and /myapp/*
let router = Router::new().merge(serve_spa("myapp"));
```

#### Serve Static Files

```rust
use axtra::routes::astro::serve_static_files;
use axum::Router;

// Serves files from ./dist, with compression and custom cache headers
let router = Router::new().merge(serve_static_files());
```

- Requests to `/` and other paths will serve files from the `./dist` directory.
- Requests for missing files will return `404.html` from the same directory.
- Cache headers are set for `_static` and `_astro` assets for optimal performance.

---

**See [`routes/health.rs`](./axtra/src/routes/health.rs) and [`routes/astro.rs`](./axtra/src/routes/astro.rs) for full implementation details.**

---

## Bouncer: IP Banning & Malicious Path Filtering

Axtra's `bouncer` middleware automatically bans IP addresses that hit known malicious or unwanted paths, helping protect your Axum app from common scanner and exploit traffic.
Enable the `bouncer` feature in your `Cargo.toml` to access the Notifier API:

### Features

- Ban IPs for a configurable duration when they access blocked paths.
- Use presets (`"wordpress"`, `"php"`, `"config"`) or custom paths for filtering.
- Customize HTTP status for banned and blocked responses.
- Set log level for event tracing (`trace`, `debug`, `info`, etc).
- Expose the banlist for observability and monitoring.

### Usage Example

```rust
use axtra::bouncer::{BouncerConfig, BouncerLayer};
use axum::{Router, routing::get};
use axum::http::StatusCode;
use tracing::Level;
use std::time::Duration;

// Create a config with presets and custom paths, and customize responses/logging
let config = BouncerConfig::from_rules(
    &["wordpress", "config"],
    &["/custom"]
)
    .duration(Duration::from_secs(1800))
    .banned_response(StatusCode::UNAUTHORIZED)
    .blocked_response(StatusCode::NOT_FOUND)
    .log_level(Level::INFO);

let layer = BouncerLayer::new(config);

let app = Router::new()
    .route("/", get(|| async { "Hello" }))
    .layer(layer);
```

### Extracting the IP Address
To ensure that we have access to the clients IP Address you must start axum with, if you do not IP address will always be None and bouncer will not work.

```rust
axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
```

instead of the usual

```rust
axum::serve(listener, app.into_make_service())
```


### Trusted Proxy
Since proxy headers can be spoofed you must opt-in to allow proxy headers for IP addresses by specifically setting trust_proxy to true.

If trust_proxy is true we will look for in descending order
- CF-conecting-ip (Cloudflare)
- X-Forwarded-For
- X-Real-IP

If no proxy headers are set or trust_proxy is false we will fallback to the connection IP address.

### Presets

Available presets for common hacker/scanner paths:
- `"wordpress"`
- `"php"`
- `"config"`

### Advanced Usage

You can also pass only presets or only custom paths:

```rust
let config = BouncerConfig::from_preset_rules(&["wordpress"]);
let config = BouncerConfig::from_custom_rules(&["/admin", "/hidden"]);
```

### Tracing & TraceLayer Integration

The bouncer middleware uses [`tracing`](https://docs.rs/tracing) to log blocked and banned events.  
You can configure the log level via `.log_level(Level::DEBUG)` or similar.

**Best Practice:**  
Place `BouncerLayer` **before** Axum's `TraceLayer` so that blocked/banned requests are logged by bouncer and not missed by TraceLayer's `on_response` hooks.

#### Example with TraceLayer

```rust
use axtra::bouncer::{BouncerConfig, BouncerLayer};
use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;

let config = BouncerConfig::from_rules(&["wordpress"], &[])
    .log_level(tracing::Level::INFO);

let app = Router::new()
    .route("/", get(|| async { "Hello" }))
    .layer(TraceLayer::new_for_http())
    .layer(BouncerLayer::new(config));
```

**Logging:**  
- Bouncer logs blocked/banned events at your chosen level.
- TraceLayer logs all requests that reach your handlers.
- Blocked/banned requests are handled and logged by bouncer, and do **not** reach TraceLayer's `on_response`.

---

**See [`bouncer/mod.rs`](./axtra/src/bouncer/mod.rs) and [`bouncer/layer.rs`](./axtra/src/bouncer/layer.rs) for full implementation details.**

## Notifier

Axtra includes a pluggable notification system for error alerts and incident tracking.
Enable provider features in your `Cargo.toml`:

```toml
[dependencies]
axtra = { version = "0.3", features = ["notify-error-slack", "notify-error-ntfy"] }
```

### NotificationManager API

The `NotificationManager` coordinates multiple providers, sending error events to all of them concurrently.

```rust
use axtra::notifier::{NotificationManager, SlackConfig, DiscordConfig, NtfyConfig};
use axtra::errors::notifiers::init_notification_manager;

// Builder pattern - recommended for explicit configuration
let manager = NotificationManager::builder()
    .with_slack(SlackConfig::new("https://hooks.slack.com/services/XXX")
        .with_mention("@oncall"))
    .with_discord(DiscordConfig::new("https://discord.com/api/webhooks/XXX"))
    .with_ntfy(NtfyConfig::new("my-app-errors"))
    .build();

init_notification_manager(manager);

// Or auto-configure from environment variables
init_notification_manager(NotificationManager::from_env());
```

### ErrorEvent

All providers receive an `ErrorEvent` with full context:

```rust
pub struct ErrorEvent {
    pub app_name: String,      // from APP_NAME env var
    pub error_code: ErrorCode, // Database, Exception, etc.
    pub message: String,       // human-readable error
    pub location: String,      // e.g., "mymodule::handler:42"
    pub timestamp: OffsetDateTime,
    pub source_error: Option<String>, // error chain for debugging
}

impl ErrorEvent {
    // Maps error_code to severity for incident systems
    pub fn severity(&self) -> &'static str; // "critical", "major", "minor", "warning"
}
```

### Custom Providers

Implement `ErrorNotifier` to create your own provider:

```rust
use axtra::notifier::{ErrorNotifier, ErrorEvent, NotifyError, NotifyFuture};
use reqwest::Client;

struct MyProvider { /* config */ }

impl ErrorNotifier for MyProvider {
    fn notify<'a>(&'a self, client: &'a Client, event: &'a ErrorEvent) -> NotifyFuture<'a> {
        Box::pin(async move {
            // Send notification using the shared HTTP client
            client.post("https://my-service.com/alert")
                .json(&serde_json::json!({
                    "message": event.message,
                    "severity": event.severity(),
                }))
                .send()
                .await?;
            Ok(())
        })
    }

    fn name(&self) -> &'static str {
        "my-provider"
    }
}
```

### NotifyError

Use `NotifyError::transient()` for retryable errors (network issues) and `NotifyError::permanent()` for configuration errors:

```rust
client.post(url)
    .send()
    .await
    .map_err(NotifyError::transient)?  // Network error - might work on retry
    .error_for_status()
    .map_err(NotifyError::permanent)?; // 4xx error - won't work on retry
```

**See [`notifier/mod.rs`](./axtra/src/notifier/mod.rs) for full API details.**

---

## Documentation

- [API Docs (docs.rs)](https://docs.rs/axtra)
- [Changelog](./CHANGELOG.md)

---

## License

MIT

---

## Contributing

PRs and issues welcome! See [CONTRIBUTING.md](./CONTRIBUTING.md).
