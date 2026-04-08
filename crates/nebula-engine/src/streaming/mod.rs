/// SRT streaming transport. Accessed via plugin SDK's
/// `platform_invoke("engine:srt:*")`, not via direct FFI.
///
/// This module is intentionally not exposed to flutter_rust_bridge.
pub(crate) mod srt_transport;
