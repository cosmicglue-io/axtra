//! Built-in notification providers.
//!
//! This module contains provider implementations for common notification services.
//! Each provider is feature-gated to avoid unnecessary dependencies.

#[cfg(feature = "notify-error-slack")]
mod slack;
#[cfg(feature = "notify-error-slack")]
pub use slack::{SlackConfig, SlackProvider};

#[cfg(feature = "notify-error-discord")]
mod discord;
#[cfg(feature = "notify-error-discord")]
pub use discord::{DiscordConfig, DiscordProvider};

#[cfg(feature = "notify-error-ntfy")]
mod ntfy;
#[cfg(feature = "notify-error-ntfy")]
pub use ntfy::{NtfyConfig, NtfyProvider};

#[cfg(feature = "notify-error-posthog")]
mod posthog;
#[cfg(feature = "notify-error-posthog")]
pub use posthog::{PosthogConfig, PosthogProvider};
