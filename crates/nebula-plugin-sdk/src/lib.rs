//! NEBULA Plugin SDK
//!
//! This crate defines the ABI contract between the NEBULA engine and plugins.
//! Plugin authors depend on this crate to access the `PluginContext` type and
//! capability definitions. The engine also depends on it to ensure ABI compatibility.
//!
//! In addition to the core ABI types, this crate provides shared orchestration
//! utilities — queue management, rate limiting, retry logic, and delivery
//! tracking — that all plugins can use.

pub mod abi;
pub mod capabilities;
pub mod context;
pub mod delivery;
pub mod manifest;
pub mod queue;
pub mod ratelimit;
pub mod retry;
