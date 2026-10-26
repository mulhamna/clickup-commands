//! `clickupc update`: upgrade through the channel that installed the binary.

use std::{path::Path, process::Command};

use anyhow::{bail, ensure, Context, Result};
use serde_json::json;

use super::Ctx;
use crate::output::{confirm, print_json};

const REPO: &str = "mulhamna/clickup-commands";

pub async fn update(ctx: &Ctx, check: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = latest_tag().await?;
    let available = is_newer(&latest, current)?;
    if ctx.json {
        print_json(
            &json!({ "current": current, "latest": latest, "update_available": available }),
        )?;
    } else if available {
        println!("clickupc {latest} is available (installed: v{current})");
    } else {
        println!("clickupc v{current} is up to date");
    }
    if check || !available {
        return Ok(());
    }

    let exe = std::env::current_exe()?.canonicalize()?;
    let exe_str = exe.to_string_lossy();
    // Windows canonical paths carry a `\\?\` prefix that PowerShell rejects.
    let exe = Path::new(exe_str.trim_start_matches(r"\\?\"));
    // npx runs a cached copy that a global npm install wouldn't replace.
    ensure!(
        !exe_str.contains("_npx"),
        "clickupc runs from the npx cache; use `npx -y @mulham28/clickupc@latest` instead"
    );
    let argv = upgrade_argv(exe, &latest, cfg!(windows));
    if !confirm(ctx.yes, &format!("Run `{}`?", argv.join(" ")))? {
        return Ok(());
    }

    // Windows can't overwrite a running exe, but it can rename it.
    let moved = cfg!(windows) && package_manager(exe).is_none();
    let old = exe.with_extension("exe.old");
    if moved {
        let _ = std::fs::remove_file(&old);
        std::fs::rename(exe, &old).context("moving the running clickupc.exe aside")?;
    }
    let status = Command::new(&argv[0]).args(&argv[1..]).status();
    let ok = matches!(&status, Ok(s) if s.success());
    if moved && !ok {
        let _ = std::fs::rename(&old, exe);
    }
    status.with_context(|| format!("running {}", argv[0]))?;
    ensure!(ok, "upgrade command failed: {}", argv.join(" "));
    Ok(())
}

async fn latest_tag() -> Result<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body: serde_json::Value = reqwest::Client::new()
        .get(&url)
        .header(
            "User-Agent",
            concat!("clickupc/", env!("CARGO_PKG_VERSION")),
        )
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    match body["tag_name"].as_str() {
        Some(tag) => Ok(tag.to_string()),
        None => bail!("no tag_name in {url}"),
    }
}

fn semver(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.trim_start_matches('v').split('.').map(|p| p.parse().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

fn is_newer(latest: &str, current: &str) -> Result<bool> {
    match (semver(latest), semver(current)) {
        (Some(l), Some(c)) => Ok(l > c),
        _ => bail!("can't compare versions {latest} and {current}"),
    }
}

/// Upgrade command of the package manager that owns `exe`, if any.
fn package_manager(exe: &Path) -> Option<&'static str> {
    let p = exe.to_string_lossy().to_lowercase().replace('\\', "/");
    [
        ("/node_modules/", "npm i -g @mulham28/clickupc@latest"),
        ("/cellar/", "brew upgrade clickupc"),
        ("/homebrew/", "brew upgrade clickupc"),
        ("/linuxbrew/", "brew upgrade clickupc"),
        ("/scoop/", "scoop update clickupc"),
        ("/.cargo/", "cargo install clickupc --force"),
    ]
    .into_iter()
    .find(|(dir, _)| p.contains(dir))
    .map(|(_, cmd)| cmd)
}

/// Package-manager upgrade, or the checksum-verifying install script into
/// the binary's own directory.
fn upgrade_argv(exe: &Path, tag: &str, windows: bool) -> Vec<String> {
    let exe_str = exe.to_string_lossy();
    let dir = exe_str.rsplit_once(['/', '\\']).map_or(".", |(d, _)| d);
    let script = format!("https://raw.githubusercontent.com/{REPO}/main/install");
    match (package_manager(exe), windows) {
        (Some(cmd), false) => vec!["sh".into(), "-c".into(), cmd.into()],
        (Some(cmd), true) => vec!["cmd".into(), "/C".into(), cmd.into()],
        (None, false) => vec![
            "env".into(),
            format!("CLICKUPC_VERSION={tag}"),
            format!("CLICKUPC_INSTALL_DIR={dir}"),
            "bash".into(),
            "-c".into(),
            format!("curl -fsSL {script}.sh | bash"),
        ],
        (None, true) => vec![
            "powershell".into(),
            "-NoProfile".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            format!(
                "& ([scriptblock]::Create((irm {script}.ps1))) -Version {tag} -InstallDir '{}' -SkipPathUpdate",
                dir.replace('\'', "''")
            ),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v1.10.0", "1.9.9").unwrap());
        assert!(!is_newer("v1.3.0", "1.3.0").unwrap());
        assert!(!is_newer("v1.2.9", "1.3.0").unwrap());
        assert!(is_newer("v1.3", "1.3.0").is_err());
    }

    #[test]
    fn picks_channel() {
        let argv = |p: &str, win| upgrade_argv(Path::new(p), "v2.0.0", win);
        assert_eq!(
            argv("/opt/homebrew/Cellar/clickupc/1.3.0/bin/clickupc", false),
            ["sh", "-c", "brew upgrade clickupc"]
        );
        assert_eq!(
            argv(
                "/usr/lib/node_modules/@mulham28/clickupc/bin/clickupc",
                false
            )[2],
            "npm i -g @mulham28/clickupc@latest"
        );
        assert_eq!(
            argv(
                r"C:\Users\me\scoop\apps\clickupc\current\clickupc.exe",
                true
            ),
            ["cmd", "/C", "scoop update clickupc"]
        );
        assert_eq!(
            argv("/home/me/.cargo/bin/clickupc", false)[2],
            "cargo install clickupc --force"
        );

        let sh = argv("/home/me/.local/bin/clickupc", false);
        assert_eq!(sh[1], "CLICKUPC_VERSION=v2.0.0");
        assert_eq!(sh[2], "CLICKUPC_INSTALL_DIR=/home/me/.local/bin");
        assert!(sh[5].ends_with("/install.sh | bash"));

        let ps = argv(r"C:\it's\clickupc.exe", true);
        assert_eq!(ps[0], "powershell");
        assert!(ps[5].contains("-Version v2.0.0 -InstallDir 'C:\\it''s'"));
    }
}
