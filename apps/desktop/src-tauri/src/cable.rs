//! One-click install of VB-CABLE (free virtual audio cable by VB-Audio):
//! downloads the official driver pack, checks its Authenticode signature and
//! runs the setup elevated. Windows still asks twice (UAC and "install this
//! device software?"), which is the user's consent for a driver.

use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::core::Core;

/// Newest first; a 404 moves on to the next.
const PACKS: &[&str] = &[
    "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack46.zip",
    "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip",
];
pub const HOMEPAGE: &str = "https://vb-audio.com/Cable/";

pub fn install(core: Arc<Core>) {
    let _ = std::thread::Builder::new().name("cable-install".into()).spawn(move || {
        core.set_cable_install("downloading");
        let result = run(&core);
        if let Err(e) = &result {
            eprintln!("VB-CABLE: {e}");
        }
        // The driver shows up a few seconds after setup returns.
        for _ in 0..20 {
            core.refresh_outputs();
            if core.outputs().iter().any(|e| e.cable.is_some()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let ok = core.outputs().iter().any(|e| e.cable.is_some());
        core.set_cable_install(if ok { "idle" } else { "failed" });
    });
}

fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn run(core: &Core) -> Result<(), String> {
    let dir: PathBuf = std::env::temp_dir().join(format!("micnexus-vbcable-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip = dir.join("pack.zip");
    // curl.exe and tar.exe ship with Windows 10 1803+.
    let downloaded = PACKS.iter().any(|url| {
        hidden(
            std::process::Command::new("curl.exe")
                .args(["-fsSL", "--retry", "2", "-o"])
                .arg(&zip)
                .arg(url),
        )
        .status()
        .is_ok_and(|s| s.success())
    });
    if !downloaded {
        return Err("download failed".into());
    }
    let unpacked = hidden(
        std::process::Command::new("tar.exe")
            .arg("-xf")
            .arg(&zip)
            .arg("-C")
            .arg(&dir),
    )
    .status()
    .is_ok_and(|s| s.success());
    if !unpacked {
        return Err("could not extract the driver pack".into());
    }
    let setup = dir.join(if cfg!(target_arch = "aarch64") {
        "VBCABLE_Setup_arm64.exe"
    } else {
        "VBCABLE_Setup_x64.exe"
    });
    if !setup.is_file() {
        return Err("setup not found in the driver pack".into());
    }
    if !signed(&setup) {
        return Err("setup signature is not valid".into());
    }
    core.set_cable_install("installing");
    run_elevated(&setup, "-i -h")?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(windows)]
fn signed(path: &std::path::Path) -> bool {
    use windows::{
        Win32::{
            Foundation::HWND,
            Security::WinTrust::{
                WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO, WTD_CHOICE_FILE,
                WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WinVerifyTrust,
            },
        },
        core::PCWSTR,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide_null();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut _) };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe { WinVerifyTrust(HWND::default(), &mut action, &mut data as *mut _ as *mut _) };
    status == 0
}

#[cfg(not(windows))]
fn signed(_path: &std::path::Path) -> bool {
    false
}

#[cfg(windows)]
trait EncodeWideNull {
    fn encode_wide_null(&self) -> Vec<u16>;
}

#[cfg(windows)]
impl EncodeWideNull for std::ffi::OsStr {
    fn encode_wide_null(&self) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        self.encode_wide().chain(Some(0)).collect()
    }
}

/// Runs `exe` through UAC ("runas") and waits for it.
#[cfg(windows)]
fn run_elevated(exe: &std::path::Path, args: &str) -> Result<(), String> {
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            System::Threading::{INFINITE, WaitForSingleObject},
            UI::{
                Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
                WindowsAndMessaging::SW_SHOWNORMAL,
            },
        },
        core::{PCWSTR, w},
    };
    let file = exe.as_os_str().encode_wide_null();
    let dir = exe
        .parent()
        .map(|p| p.as_os_str().encode_wide_null())
        .unwrap_or_else(|| vec![0]);
    let params: Vec<u16> = args.encode_utf16().chain(Some(0)).collect();
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        lpDirectory: PCWSTR(dir.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| format!("could not start setup: {e}"))?;
    if !info.hProcess.is_invalid() {
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            let _ = CloseHandle(info.hProcess);
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn run_elevated(_exe: &std::path::Path, _args: &str) -> Result<(), String> {
    Err("Windows only".into())
}
