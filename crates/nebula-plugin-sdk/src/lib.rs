//! NEBULA Plugin SDK
//!
//! This crate defines the ABI contract between the NEBULA engine and plugins.
//! Plugin authors depend on this crate to access the `PluginContext` type and
//! capability definitions. The engine also depends on it to ensure ABI compatibility.

pub mod abi;
pub mod capabilities;
pub mod context;
pub mod manifest;
