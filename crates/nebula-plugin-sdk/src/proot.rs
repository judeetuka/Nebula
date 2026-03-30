//! Proot Linux Bridge SDK convenience functions.
//!
//! Provides high-level wrappers around `platform_invoke("engine:proot:*", ...)`
//! so plugin developers can access proot distros without manual C-ABI calls.

use crate::context::PluginContext;
use serde::{Deserialize, Serialize};

/// Result of executing a command inside a proot environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Information about an available Linux distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distro {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size_mb: u32,
}

/// Information about an installed Linux distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledDistro {
    pub id: String,
    pub name: String,
    pub version: String,
    pub path: String,
}

/// Check if the proot binary is available on this device.
pub fn check_proot(ctx: &PluginContext) -> Result<bool, String> {
    let resp = invoke_engine(ctx, "proot:check", "{}")?;
    let v: serde_json::Value = serde_json::from_str(&resp).map_err(|e| e.to_string())?;
    Ok(v["installed"].as_bool().unwrap_or(false))
}

/// Download the proot binary for the current device architecture.
pub fn download_proot(ctx: &PluginContext) -> Result<(), String> {
    invoke_engine(ctx, "proot:download", "{}")?;
    Ok(())
}

/// List all available Linux distributions in the catalog.
pub fn list_distros(ctx: &PluginContext) -> Result<Vec<Distro>, String> {
    let resp = invoke_engine(ctx, "proot:list_distros", "{}")?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Install a Linux distribution by ID (downloads and extracts rootfs).
pub fn install_distro(ctx: &PluginContext, distro_id: &str) -> Result<(), String> {
    let args = serde_json::json!({ "distro": distro_id }).to_string();
    invoke_engine(ctx, "proot:install", &args)?;
    Ok(())
}

/// Remove an installed Linux distribution.
pub fn remove_distro(ctx: &PluginContext, distro_id: &str) -> Result<(), String> {
    let args = serde_json::json!({ "distro": distro_id }).to_string();
    invoke_engine(ctx, "proot:remove", &args)?;
    Ok(())
}

/// List all currently installed Linux distributions.
pub fn list_installed(ctx: &PluginContext) -> Result<Vec<InstalledDistro>, String> {
    let resp = invoke_engine(ctx, "proot:list_installed", "{}")?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Execute a command inside a proot Linux environment.
///
/// # Arguments
/// * `distro_id` - ID of the installed distro (e.g., "alpine", "ubuntu")
/// * `command` - Shell command to execute (e.g., "cat /etc/os-release")
/// * `cwd` - Working directory inside the rootfs (default: "/")
pub fn exec(
    ctx: &PluginContext,
    distro_id: &str,
    command: &str,
    cwd: Option<&str>,
) -> Result<ExecResult, String> {
    let args = serde_json::json!({
        "distro": distro_id,
        "command": command,
        "cwd": cwd.unwrap_or("/"),
    })
    .to_string();
    let resp = invoke_engine(ctx, "proot:exec", &args)?;
    serde_json::from_str(&resp).map_err(|e| format!("parse error: {e}"))
}

/// Execute a command and return only stdout (convenience wrapper).
pub fn exec_stdout(ctx: &PluginContext, distro_id: &str, command: &str) -> Result<String, String> {
    let result = exec(ctx, distro_id, command, None)?;
    if result.exit_code != 0 {
        return Err(format!(
            "Command failed (exit {}): {}",
            result.exit_code, result.stderr
        ));
    }
    Ok(result.stdout)
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

fn invoke_engine(ctx: &PluginContext, command: &str, args: &str) -> Result<String, String> {
    let capability = format!("engine:{command}");
    let method = "";
    let mut result_buf = vec![0u8; 65536];
    let ret = (ctx.platform_invoke)(
        ctx.host_data,
        capability.as_ptr(),
        capability.len(),
        method.as_ptr(),
        method.len(),
        args.as_ptr(),
        args.len(),
        result_buf.as_mut_ptr(),
        result_buf.len(),
    );
    if ret < 0 {
        Err(format!("engine invoke '{command}' failed: {ret}"))
    } else {
        let s = std::str::from_utf8(&result_buf[..ret as usize])
            .map_err(|e| format!("Invalid UTF-8: {e}"))?;
        Ok(s.to_string())
    }
}
