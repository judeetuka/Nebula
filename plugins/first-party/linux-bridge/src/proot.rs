//! Proot management and Linux distro orchestration.
//!
//! Handles proot binary verification, distro catalog, rootfs installation,
//! removal, listing, command execution, and shell session setup.

use crate::common;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distro {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size_mb: u32,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProotConfig {
    pub proot_path: String,
    pub rootfs_base: String,
    pub arch: String,
}

impl Default for ProotConfig {
    fn default() -> Self {
        Self {
            proot_path: "/data/data/com.nebula.node/files/proot/proot".into(),
            rootfs_base: "/data/data/com.nebula.node/files/proot/rootfs".into(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Distro catalog
// ---------------------------------------------------------------------------

/// proot-distro release version used for rootfs tarballs.
const PROOT_DISTRO_VERSION: &str = "v4.18.0";

fn distro_catalog() -> Vec<Distro> {
    let base = format!(
        "https://github.com/termux/proot-distro/releases/download/{}",
        PROOT_DISTRO_VERSION
    );
    vec![
        Distro {
            id: "alpine".into(),
            name: "Alpine Linux".into(),
            version: "3.21".into(),
            size_mb: 8,
            url: format!("{}/alpine-aarch64-pd-{}.tar.xz", base, PROOT_DISTRO_VERSION),
        },
        Distro {
            id: "ubuntu".into(),
            name: "Ubuntu LTS".into(),
            version: "24.04 Noble".into(),
            size_mb: 50,
            url: format!(
                "{}/ubuntu-noble-aarch64-pd-{}.tar.xz",
                base, PROOT_DISTRO_VERSION
            ),
        },
        Distro {
            id: "debian".into(),
            name: "Debian Stable".into(),
            version: "12 Bookworm".into(),
            size_mb: 45,
            url: format!(
                "{}/debian-bookworm-aarch64-pd-{}.tar.xz",
                base, PROOT_DISTRO_VERSION
            ),
        },
        Distro {
            id: "debian-trixie".into(),
            name: "Debian Testing".into(),
            version: "13 Trixie".into(),
            size_mb: 50,
            url: format!(
                "{}/debian-trixie-aarch64-pd-{}.tar.xz",
                base, PROOT_DISTRO_VERSION
            ),
        },
        Distro {
            id: "archlinux".into(),
            name: "Arch Linux".into(),
            version: "Rolling".into(),
            size_mb: 120,
            url: format!(
                "{}/archlinux-aarch64-pd-{}.tar.xz",
                base, PROOT_DISTRO_VERSION
            ),
        },
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_config() -> ProotConfig {
    match common::get_state("proot_config") {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => ProotConfig::default(),
    }
}

fn rootfs_path(config: &ProotConfig, distro_id: &str) -> String {
    format!("{}/{}", config.rootfs_base, distro_id)
}

fn get_installed_list() -> Vec<String> {
    match common::get_state("installed_distros") {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn shell_escape(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

// ---------------------------------------------------------------------------
// Action handlers
// ---------------------------------------------------------------------------

/// Check if the proot binary is available at the expected path.
pub fn check_proot(_params: &serde_json::Value) -> Result<String, String> {
    let config = get_config();

    let check_args = serde_json::json!({
        "path": config.proot_path
    })
    .to_string();

    let cap = "engine:fs:exists";
    let installed = match common::invoke(cap, &check_args) {
        Ok(resp) => {
            let v: serde_json::Value =
                serde_json::from_str(&resp).unwrap_or(serde_json::Value::Bool(false));
            v["exists"].as_bool().unwrap_or(false)
        }
        Err(_) => false,
    };

    let result = serde_json::json!({
        "installed": installed,
        "path": config.proot_path,
        "arch": config.arch,
    });
    Ok(result.to_string())
}

/// Download the proot binary for the current architecture.
pub fn download_proot(params: &serde_json::Value) -> Result<String, String> {
    let config = get_config();
    let arch = params["arch"].as_str().unwrap_or(&config.arch).to_string();

    let download_url = format!(
        "https://github.com/proot-me/proot/releases/download/v5.4.0/proot-v5.4.0-{}-static",
        arch
    );

    let args = serde_json::json!({
        "url": download_url,
        "dest": config.proot_path,
        "executable": true,
    })
    .to_string();

    common::log(
        common::log_level::INFO,
        &format!("Downloading proot for {arch} from {download_url}"),
    );

    let resp = common::invoke("engine:download", &args)?;

    let result = serde_json::json!({
        "success": true,
        "path": config.proot_path,
        "arch": arch,
        "download_response": resp,
    });
    Ok(result.to_string())
}

/// List all available distros from the catalog.
pub fn list_distros(_params: &serde_json::Value) -> Result<String, String> {
    let catalog = distro_catalog();
    serde_json::to_string(&catalog).map_err(|e| format!("Serialization error: {e}"))
}

/// Install a distro by downloading and extracting its rootfs.
pub fn install_distro(params: &serde_json::Value) -> Result<String, String> {
    let distro_id = params["distro"]
        .as_str()
        .ok_or_else(|| "Missing required field: distro".to_string())?;

    let config = get_config();
    let catalog = distro_catalog();

    let distro = catalog
        .iter()
        .find(|d| d.id == distro_id)
        .ok_or_else(|| format!("Unknown distro: {distro_id}"))?;

    let install_path = params["install_path"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| rootfs_path(&config, distro_id));

    common::log(
        common::log_level::INFO,
        &format!("Installing {} to {}", distro.name, install_path),
    );

    // Step 1: Download the rootfs tarball to a temp location
    let tarball_path = format!("{}.tar.gz", install_path);
    let download_args = serde_json::json!({
        "url": distro.url,
        "dest": tarball_path,
    })
    .to_string();

    common::invoke("engine:download", &download_args)?;

    // Step 2: Extract the tarball
    let extract_args = serde_json::json!({
        "archive": tarball_path,
        "dest": install_path,
    })
    .to_string();

    common::invoke("engine:extract", &extract_args)?;

    // Step 3: Clean up the tarball
    let cleanup_args = serde_json::json!({
        "path": tarball_path,
    })
    .to_string();

    let _ = common::invoke("engine:fs:remove", &cleanup_args);

    // Step 4: Record installation in state
    let mut installed = get_installed_list();
    if !installed.contains(&distro_id.to_string()) {
        installed.push(distro_id.to_string());
        let _ = common::set_state(
            "installed_distros",
            &serde_json::to_string(&installed).unwrap_or_default(),
        );
    }

    let result = serde_json::json!({
        "success": true,
        "distro": distro_id,
        "name": distro.name,
        "path": install_path,
        "size_mb": distro.size_mb,
    });
    Ok(result.to_string())
}

/// Remove an installed distro's rootfs directory.
pub fn remove_distro(params: &serde_json::Value) -> Result<String, String> {
    let distro_id = params["distro"]
        .as_str()
        .ok_or_else(|| "Missing required field: distro".to_string())?;

    let config = get_config();
    let path = rootfs_path(&config, distro_id);

    common::log(
        common::log_level::INFO,
        &format!("Removing distro {distro_id} at {path}"),
    );

    let args = serde_json::json!({
        "path": path,
        "recursive": true,
    })
    .to_string();

    common::invoke("engine:fs:remove", &args)?;

    // Update installed list
    let mut installed = get_installed_list();
    installed.retain(|d| d != distro_id);
    let _ = common::set_state(
        "installed_distros",
        &serde_json::to_string(&installed).unwrap_or_default(),
    );

    let result = serde_json::json!({
        "success": true,
        "distro": distro_id,
    });
    Ok(result.to_string())
}

/// List all currently installed distros.
pub fn list_installed(_params: &serde_json::Value) -> Result<String, String> {
    let config = get_config();
    let installed = get_installed_list();
    let catalog = distro_catalog();

    let mut entries = Vec::new();
    for id in &installed {
        let path = rootfs_path(&config, id);
        let distro_info = catalog.iter().find(|d| d.id == *id);
        entries.push(serde_json::json!({
            "id": id,
            "name": distro_info.map(|d| d.name.as_str()).unwrap_or("Unknown"),
            "version": distro_info.map(|d| d.version.as_str()).unwrap_or("?"),
            "path": path,
        }));
    }

    serde_json::to_string(&entries).map_err(|e| format!("Serialization error: {e}"))
}

/// Execute a command inside a proot environment.
pub fn exec_command(params: &serde_json::Value) -> Result<String, String> {
    let distro_id = params["distro"]
        .as_str()
        .ok_or_else(|| "Missing required field: distro".to_string())?;
    let command = params["command"]
        .as_str()
        .ok_or_else(|| "Missing required field: command".to_string())?;

    let config = get_config();
    let rfs = rootfs_path(&config, distro_id);
    let working_dir = params["cwd"].as_str().unwrap_or("/");

    let proot_cmd = format!(
        "{proot} -0 -r {rootfs} -b /dev -b /proc -b /sys -w {cwd} /bin/sh -c {cmd}",
        proot = config.proot_path,
        rootfs = rfs,
        cwd = working_dir,
        cmd = shell_escape(command),
    );

    common::log(
        common::log_level::DEBUG,
        &format!("proot exec: {proot_cmd}"),
    );

    let args = serde_json::json!({
        "command": proot_cmd,
    })
    .to_string();

    let resp = common::invoke("android:exec", &args)?;

    let exec_resp: serde_json::Value =
        serde_json::from_str(&resp).unwrap_or(serde_json::Value::Null);

    let result = ExecResult {
        exit_code: exec_resp["exit_code"].as_i64().unwrap_or(-1) as i32,
        stdout: exec_resp["stdout"].as_str().unwrap_or("").to_string(),
        stderr: exec_resp["stderr"].as_str().unwrap_or("").to_string(),
    };

    serde_json::to_string(&result).map_err(|e| format!("Serialization error: {e}"))
}

/// Start an interactive shell session (returns proot command and session info).
pub fn start_shell(params: &serde_json::Value) -> Result<String, String> {
    let distro_id = params["distro"]
        .as_str()
        .ok_or_else(|| "Missing required field: distro".to_string())?;

    let config = get_config();
    let rfs = rootfs_path(&config, distro_id);
    let shell = params["shell"].as_str().unwrap_or("/bin/sh");

    let proot_cmd = format!(
        "{proot} -0 -r {rootfs} -b /dev -b /proc -b /sys -w / {shell}",
        proot = config.proot_path,
        rootfs = rfs,
        shell = shell,
    );

    let session_id = format!("proot-{}-{}", distro_id, common::now_ms());

    common::log(
        common::log_level::INFO,
        &format!("Shell session {session_id} prepared for {distro_id}"),
    );

    let result = serde_json::json!({
        "session_id": session_id,
        "distro": distro_id,
        "command": proot_cmd,
        "note": "Interactive shells require PTY allocation via android:pty bridge. Use the command field to start manually or via engine PTY support.",
    });
    Ok(result.to_string())
}
