use crate::catalog::{HarnessEntry, LaunchTarget};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub(crate) struct DetectEnv {
    pub local_app_data: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub program_data: Option<PathBuf>,
}

impl DetectEnv {
    fn from_process() -> Self {
        Self {
            local_app_data: env_path("LOCALAPPDATA"),
            program_files: env_path("ProgramFiles"),
            app_data: env_path("APPDATA"),
            program_data: env_path("ProgramData"),
        }
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

fn first_existing_file(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.is_file()).cloned()
}

fn find_start_menu_shortcut(env: &DetectEnv, names: &[&str]) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(appdata) = &env.app_data {
        roots.push(appdata.join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Some(program_data) = &env.program_data {
        roots.push(program_data.join("Microsoft/Windows/Start Menu/Programs"));
    }
    for root in roots {
        for name in names {
            let p = root.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Pure filesystem + Win32 Registry MSIX PackageFamily + AUMID lookup (ZERO PowerShell).
fn find_msix_package(
    env: &DetectEnv,
    family_prefixes: &[&str],
    fallback_app_id: &str,
) -> Option<LaunchTarget> {
    let windows_apps = env.local_app_data.as_ref()?.join("Microsoft/WindowsApps");
    let entries = std::fs::read_dir(&windows_apps).ok()?;
    for entry in entries.flatten() {
        let fname = entry.file_name().to_string_lossy().into_owned();
        let low = fname.to_ascii_lowercase();
        let matches = family_prefixes
            .iter()
            .any(|p| low.starts_with(&p.to_ascii_lowercase()))
            && fname.contains('_');
        if matches && entry.path().is_dir() {
            let app_id = resolve_msix_app_id(&fname)
                .unwrap_or_else(|| fallback_app_id.to_owned());
            let aumid = format!("{fname}!{app_id}");
            return Some(LaunchTarget::MsixAumid {
                package_family: fname,
                aumid,
            });
        }
    }
    None
}

#[cfg(windows)]
fn resolve_msix_app_id(package_family: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    };

    let prefix = package_family.split('_').next().unwrap_or(package_family);
    let prefix_low = prefix.to_ascii_lowercase();
    let subkey: Vec<u16> =
        "Software\\Classes\\ActivatableClasses\\Package\0"
            .encode_utf16()
            .collect();
    let mut hkey: HKEY = std::ptr::null_mut();
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
    if rc != 0 {
        return None;
    }

    let mut matched_pkg: Option<String> = None;
    let mut idx = 0u32;
    let mut buf = [0u16; 512];
    loop {
        let mut len = buf.len() as u32;
        let er = unsafe {
            RegEnumKeyExW(
                hkey,
                idx,
                buf.as_mut_ptr(),
                &mut len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if er != 0 {
            break;
        }
        let name = String::from_utf16_lossy(&buf[..len as usize]);
        if name.to_ascii_lowercase().starts_with(&prefix_low) {
            matched_pkg = Some(name);
            break;
        }
        idx += 1;
    }
    unsafe { RegCloseKey(hkey) };

    let pkg_full = matched_pkg?;
    let server_subkey: Vec<u16> = format!(
        "Software\\Classes\\ActivatableClasses\\Package\\{pkg_full}\\Server\0"
    )
    .encode_utf16()
    .collect();
    let mut skey: HKEY = std::ptr::null_mut();
    let rc2 =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, server_subkey.as_ptr(), 0, KEY_READ, &mut skey) };
    if rc2 != 0 {
        return None;
    }
    let mut s_len = buf.len() as u32;
    let er2 = unsafe {
        RegEnumKeyExW(
            skey,
            0,
            buf.as_mut_ptr(),
            &mut s_len,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    unsafe { RegCloseKey(skey) };
    if er2 == 0 && s_len > 0 {
        let raw = String::from_utf16_lossy(&buf[..s_len as usize]);
        if !raw.is_empty() && !raw.contains('\\') {
            return Some(raw);
        }
    }
    None
}

#[cfg(not(windows))]
fn resolve_msix_app_id(_package_family: &str) -> Option<String> {
    None
}

fn detect_exe_or_shortcut(
    env: &DetectEnv,
    exe_rel_paths: &[&str],
    shortcut_names: &[&str],
) -> Option<LaunchTarget> {
    let mut candidates = Vec::new();
    if let Some(la) = &env.local_app_data {
        for rel in exe_rel_paths {
            candidates.push(la.join(rel));
        }
    }
    if let Some(pf) = &env.program_files {
        for rel in exe_rel_paths {
            let trimmed = rel.strip_prefix("Programs/").unwrap_or(rel);
            candidates.push(pf.join(trimmed));
        }
    }
    if let Some(exe) = first_existing_file(&candidates) {
        return Some(LaunchTarget::Exe(exe));
    }
    if let Some(lnk) = find_start_menu_shortcut(env, shortcut_names) {
        return Some(LaunchTarget::Shortcut(lnk));
    }
    None
}

fn detect_windows_apps_alias(env: &DetectEnv, exe_name: &str) -> Option<LaunchTarget> {
    let p = env
        .local_app_data
        .as_ref()?
        .join("Microsoft/WindowsApps")
        .join(exe_name);
    if Path::new(&p).exists() {
        Some(LaunchTarget::Exe(p))
    } else {
        None
    }
}

pub fn detect_builtin_harnesses() -> Vec<HarnessEntry> {
    detect_builtin_harnesses_in(&DetectEnv::from_process())
}

pub(crate) fn detect_builtin_harnesses_in(env: &DetectEnv) -> Vec<HarnessEntry> {
    let claude_target = find_msix_package(env, &["Claude_"], "Claude")
        .or_else(|| detect_windows_apps_alias(env, "claude-desktop.exe"))
        .or_else(|| {
            detect_exe_or_shortcut(
                env,
                &["AnthropicClaude/claude.exe", "Programs/Claude/Claude.exe"],
                &["Claude.lnk"],
            )
        });

    let codex_target = find_msix_package(env, &["OpenAI.Codex_"], "App").or_else(|| {
        detect_exe_or_shortcut(
            env,
            &[
                "Programs/Codex/Codex.exe",
                "Programs/Codex Web GPT/Codex Web GPT.exe",
            ],
            &["Codex.lnk"],
        )
    });

    let opencode_target = detect_exe_or_shortcut(
        env,
        &[
            "Programs/ai.opencode.desktop/OpenCode.exe",
            "Programs/OpenCode/OpenCode.exe",
        ],
        &["OpenCode.lnk"],
    );

    let zcode_target = detect_exe_or_shortcut(
        env,
        &["Programs/ZCode/ZCode.exe", "Programs/zcode/ZCode.exe"],
        &["ZCode.lnk"],
    );

    let antigravity_target = detect_exe_or_shortcut(
        env,
        &[
            "Programs/antigravity/Antigravity.exe",
            "Programs/Antigravity/Antigravity.exe",
        ],
        &["Antigravity.lnk"],
    );

    vec![
        HarnessEntry {
            id: "claude".to_owned(),
            label: "Claude".to_owned(),
            hook_agent: Some("claude".to_owned()),
            target: claude_target,
            custom: false,
            process_names: vec!["claude.exe".to_owned(), "claude-desktop.exe".to_owned()],
        },
        HarnessEntry {
            id: "codex".to_owned(),
            label: "Codex".to_owned(),
            hook_agent: Some("codex".to_owned()),
            target: codex_target,
            custom: false,
            process_names: vec!["Codex.exe".to_owned(), "codex.exe".to_owned()],
        },
        HarnessEntry {
            id: "opencode".to_owned(),
            label: "OpenCode".to_owned(),
            hook_agent: Some("opencode".to_owned()),
            target: opencode_target,
            custom: false,
            process_names: vec!["OpenCode.exe".to_owned(), "opencode-desktop.exe".to_owned()],
        },
        HarnessEntry {
            id: "zcode".to_owned(),
            label: "ZCode".to_owned(),
            hook_agent: Some("zcode".to_owned()),
            target: zcode_target,
            custom: false,
            process_names: vec!["ZCode.exe".to_owned()],
        },
        HarnessEntry {
            id: "antigravity".to_owned(),
            label: "Antigravity".to_owned(),
            hook_agent: Some("antigravity".to_owned()),
            target: antigravity_target,
            custom: false,
            process_names: vec!["Antigravity.exe".to_owned()],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_all_five_builtin_harnesses_in_temp_dir_without_host_installs() {
        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("LocalAppData");
        let prog_files = dir.path().join("ProgramFiles");
        let app_data = dir.path().join("AppData");
        let prog_data = dir.path().join("ProgramData");

        std::fs::create_dir_all(local.join("Microsoft/WindowsApps/Claude_test123")).unwrap();
        std::fs::create_dir_all(local.join("Microsoft/WindowsApps/OpenAI.Codex_test456")).unwrap();
        std::fs::create_dir_all(local.join("Programs/OpenCode")).unwrap();
        std::fs::write(local.join("Programs/OpenCode/OpenCode.exe"), b"MZ").unwrap();
        std::fs::create_dir_all(prog_files.join("ZCode")).unwrap();
        std::fs::write(prog_files.join("ZCode/ZCode.exe"), b"MZ").unwrap();
        std::fs::create_dir_all(app_data.join("Microsoft/Windows/Start Menu/Programs")).unwrap();
        std::fs::write(
            app_data.join("Microsoft/Windows/Start Menu/Programs/Antigravity.lnk"),
            b"LNK",
        )
        .unwrap();

        let env = DetectEnv {
            local_app_data: Some(local.clone()),
            program_files: Some(prog_files.clone()),
            app_data: Some(app_data.clone()),
            program_data: Some(prog_data),
        };
        let list = detect_builtin_harnesses_in(&env);
        let ids: Vec<&str> = list.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "codex", "opencode", "zcode", "antigravity"]);

        assert!(matches!(
            &list[0].target,
            Some(LaunchTarget::MsixAumid { package_family, .. }) if package_family == "Claude_test123"
        ));
        assert!(matches!(
            &list[1].target,
            Some(LaunchTarget::MsixAumid { package_family, .. }) if package_family == "OpenAI.Codex_test456"
        ));
        assert_eq!(
            list[2].target,
            Some(LaunchTarget::Exe(local.join("Programs/OpenCode/OpenCode.exe")))
        );
        assert_eq!(
            list[3].target,
            Some(LaunchTarget::Exe(prog_files.join("ZCode/ZCode.exe")))
        );
        assert_eq!(
            list[4].target,
            Some(LaunchTarget::Shortcut(
                app_data.join("Microsoft/Windows/Start Menu/Programs/Antigravity.lnk")
            ))
        );
    }

    #[test]
    fn returns_none_targets_for_empty_temp_env() {
        let dir = tempfile::tempdir().unwrap();
        let env = DetectEnv {
            local_app_data: Some(dir.path().join("local")),
            program_files: Some(dir.path().join("pf")),
            app_data: Some(dir.path().join("appdata")),
            program_data: Some(dir.path().join("progdata")),
        };
        let list = detect_builtin_harnesses_in(&env);
        assert_eq!(list.len(), 5);
        assert!(list.iter().all(|h| h.target.is_none()));
    }

    #[test]
    fn detects_all_five_builtin_harnesses_without_powershell() {
        let list = detect_builtin_harnesses();
        let ids: Vec<&str> = list.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, vec!["claude", "codex", "opencode", "zcode", "antigravity"]);
    }
}
