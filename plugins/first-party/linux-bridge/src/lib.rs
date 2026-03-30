mod common;
mod proot;

use common::CTX;
use nebula_plugin_sdk::context::PluginContext;
use std::ffi::CString;
use std::sync::atomic::Ordering;

#[no_mangle]
pub extern "C" fn nebula_plugin_init(ctx: *mut PluginContext) {
    CTX.store(ctx, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_execute(
    input: *const u8, input_len: u32, output: *mut u8, output_len: u32,
) -> i32 {
    let input_slice = unsafe { std::slice::from_raw_parts(input, input_len as usize) };
    let action_len = u32::from_le_bytes(input_slice[..4].try_into().unwrap_or([0; 4])) as usize;
    let action = std::str::from_utf8(&input_slice[4..4 + action_len]).unwrap_or("");
    let args_raw = std::str::from_utf8(&input_slice[4 + action_len..]).unwrap_or("{}");
    let params: serde_json::Value = serde_json::from_str(args_raw).unwrap_or_default();

    let result = dispatch(action, &params);

    match result {
        Ok(data) => write_result(data.as_bytes(), output, output_len),
        Err(e) => {
            let err_json = serde_json::json!({ "error": e }).to_string();
            write_result(err_json.as_bytes(), output, output_len)
        }
    }
}

#[no_mangle]
pub extern "C" fn nebula_plugin_shutdown() {
    CTX.store(std::ptr::null_mut(), Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn nebula_plugin_info() -> *const std::ffi::c_char {
    let info = r#"{"id":"linux-bridge","name":"Linux Bridge","version":"0.1.0","description":"Run Linux distributions on Android via proot","author":"HexiCore","actions":["check_proot","download_proot","list_distros","install_distro","remove_distro","list_installed","exec","start_shell"]}"#;
    CString::new(info).unwrap().into_raw()
}

fn dispatch(action: &str, params: &serde_json::Value) -> Result<String, String> {
    match action {
        "check_proot" => proot::check_proot(params),
        "download_proot" => proot::download_proot(params),
        "list_distros" => proot::list_distros(params),
        "install_distro" => proot::install_distro(params),
        "remove_distro" => proot::remove_distro(params),
        "list_installed" => proot::list_installed(params),
        "exec" => proot::exec_command(params),
        "start_shell" => proot::start_shell(params),
        _ => Err(format!("Unknown action: {action}")),
    }
}

fn write_result(data: &[u8], output: *mut u8, output_len: u32) -> i32 {
    let len = data.len().min(output_len as usize);
    if !output.is_null() && len > 0 {
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), output, len) };
    }
    len as i32
}
