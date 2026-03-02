//! ABI function type definitions for the plugin boundary.
//!
//! These types define the `extern "C"` function signatures that plugins must
//! export. The engine resolves these symbols at load time via `dlopen`/`dlsym`.

use crate::context::PluginContext;

/// Plugin initialization function signature.
/// Called once after dlopen. Returns 0 on success, non-zero on error.
pub type PluginInitFn = unsafe extern "C" fn(ctx: *const PluginContext) -> i32;

/// Plugin execution function signature.
/// Called to process a task/command. Returns bytes written to output, or -1 on error.
pub type PluginExecuteFn = unsafe extern "C" fn(
    input_ptr: *const u8,
    input_len: usize,
    output_ptr: *mut u8,
    output_len: usize,
) -> i32;

/// Plugin shutdown function signature.
/// Called before dlclose. Returns 0 on success.
pub type PluginShutdownFn = unsafe extern "C" fn() -> i32;

/// Plugin version function signature (optional).
/// Returns a null-terminated C string with the plugin version.
pub type PluginVersionFn = unsafe extern "C" fn() -> *const std::ffi::c_char;

/// Plugin info function signature (optional).
/// Returns a null-terminated JSON string with the plugin manifest.
pub type PluginInfoFn = unsafe extern "C" fn() -> *const std::ffi::c_char;

/// Required symbol names that every plugin must export.
pub const SYMBOL_INIT: &[u8] = b"nebula_plugin_init";
pub const SYMBOL_EXECUTE: &[u8] = b"nebula_plugin_execute";
pub const SYMBOL_SHUTDOWN: &[u8] = b"nebula_plugin_shutdown";

/// Optional symbol names.
pub const SYMBOL_VERSION: &[u8] = b"nebula_plugin_version";
pub const SYMBOL_INFO: &[u8] = b"nebula_plugin_info";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_symbol_names() {
        assert_eq!(SYMBOL_INIT, b"nebula_plugin_init");
        assert_eq!(SYMBOL_EXECUTE, b"nebula_plugin_execute");
        assert_eq!(SYMBOL_SHUTDOWN, b"nebula_plugin_shutdown");
    }

    #[test]
    fn test_optional_symbol_names() {
        assert_eq!(SYMBOL_VERSION, b"nebula_plugin_version");
        assert_eq!(SYMBOL_INFO, b"nebula_plugin_info");
    }

    #[test]
    fn test_symbol_names_are_valid_utf8() {
        assert!(std::str::from_utf8(SYMBOL_INIT).is_ok());
        assert!(std::str::from_utf8(SYMBOL_EXECUTE).is_ok());
        assert!(std::str::from_utf8(SYMBOL_SHUTDOWN).is_ok());
        assert!(std::str::from_utf8(SYMBOL_VERSION).is_ok());
        assert!(std::str::from_utf8(SYMBOL_INFO).is_ok());
    }
}
