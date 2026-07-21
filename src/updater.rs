use eframe::egui;

#[derive(Clone)]
pub enum UiEvent {
    UpdateAvailable {
        version: String,
        html_url: String,
        download_url: String,
    },
    UpToDate,
    UpdateCheckFailed(String),
    UpdateApplied,
    UpdateInstallFailed(String),
    LinkTitleFetched {
        shape_id: usize,
        url: String,
        title: String,
    },
}

#[derive(Default, Clone)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpdateAvailable {
        version: String,
        html_url: String,
        download_url: String,
    },
    UpToDate,
    Updating,
    UpdateDone,
    Failed(String),
}

pub fn platform_asset_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "kugel-macos.app.tar.gz"
    }
    #[cfg(target_os = "linux")]
    {
        "kugel-linux"
    }
    #[cfg(target_os = "windows")]
    {
        "kugel-windows.exe"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        ""
    }
}

pub fn spawn_update_check(ui_tx: std::sync::mpsc::Sender<UiEvent>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = (|| -> Result<(String, String, String), String> {
            let client = reqwest::blocking::Client::builder()
                .user_agent("kugel-updater")
                .build()
                .map_err(|e| e.to_string())?;
            let resp: serde_json::Value = client
                .get("https://api.github.com/repos/salernoelia/kugel/releases/latest")
                .send()
                .map_err(|e| e.to_string())?
                .json()
                .map_err(|e| e.to_string())?;
            let tag = resp["tag_name"]
                .as_str()
                .ok_or("Missing tag_name")?
                .trim_start_matches('v')
                .to_string();
            let html_url = resp["html_url"]
                .as_str()
                .ok_or("Missing html_url")?
                .to_string();
            let asset_name = platform_asset_name();
            let download_url = resp["assets"]
                .as_array()
                .and_then(|assets| {
                    assets
                        .iter()
                        .find(|a| a["name"].as_str().map(|n| n == asset_name).unwrap_or(false))
                })
                .and_then(|a| a["browser_download_url"].as_str())
                .unwrap_or("")
                .to_string();
            Ok((tag, html_url, download_url))
        })();

        match result {
            Ok((latest, html_url, download_url)) => {
                let current = env!("CARGO_PKG_VERSION");
                if latest != current {
                    let _ = ui_tx.send(UiEvent::UpdateAvailable {
                        version: latest,
                        html_url,
                        download_url,
                    });
                } else {
                    let _ = ui_tx.send(UiEvent::UpToDate);
                }
            }
            Err(e) => {
                let _ = ui_tx.send(UiEvent::UpdateCheckFailed(e));
            }
        }
        ctx.request_repaint();
    });
}

pub fn do_self_update(download_url: &str) -> Result<(), String> {
    let tmp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let archive_path = tmp_dir.path().join("kugel-update.tar.gz");
        let mut archive_file = std::fs::File::create(&archive_path).map_err(|e| e.to_string())?;
        let client = reqwest::blocking::Client::builder()
            .user_agent("kugel-updater")
            .build()
            .map_err(|e| e.to_string())?;
        let bytes = client
            .get(download_url)
            .send()
            .and_then(|r| r.bytes())
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut bytes.as_ref(), &mut archive_file).map_err(|e| e.to_string())?;

        let file = std::fs::File::open(&archive_path).map_err(|e| e.to_string())?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        let extract_to = tmp_dir.path().join("kugel_bin");
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let entry_path = entry.path().map_err(|e| e.to_string())?;
            let file_name = entry_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if file_name == "kugel" {
                entry.unpack(&extract_to).map_err(|e| e.to_string())?;
                break;
            }
        }

        if !extract_to.exists() {
            return Err("Could not find kugel binary inside update archive".to_string());
        }

        self_replace::self_replace(&extract_to).map_err(|e| e.to_string())?;

        // Re-sign the .app bundle after binary replacement.
        if let Ok(exe_path) = std::env::current_exe() {
            let mut current = exe_path.as_path();
            let mut bundle_path: Option<std::path::PathBuf> = None;
            loop {
                if current.extension().and_then(|e| e.to_str()) == Some("app") {
                    bundle_path = Some(current.to_path_buf());
                    break;
                }
                match current.parent() {
                    Some(p) => current = p,
                    None => break,
                }
            }
            if let Some(bundle) = bundle_path {
                let _ = std::process::Command::new("codesign")
                    .args(["-s", "-", "--deep", "--force"])
                    .arg(&bundle)
                    .output();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let bin_path = tmp_dir.path().join("kugel_new");
        let client = reqwest::blocking::Client::builder()
            .user_agent("kugel-updater")
            .build()
            .map_err(|e| e.to_string())?;
        let bytes = client
            .get(download_url)
            .send()
            .and_then(|r| r.bytes())
            .map_err(|e| e.to_string())?;
        std::fs::write(&bin_path, &bytes).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bin_path)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bin_path, perms).map_err(|e| e.to_string())?;
        }

        self_replace::self_replace(&bin_path).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_asset_name_not_empty() {
        assert!(!platform_asset_name().is_empty());
    }
}
