pub mod adb;
pub mod audio;
pub mod bluetooth;
pub mod cable;
pub mod core;
pub mod lan;
pub mod protocol;
pub mod settings;

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    webview::{PermissionKind, PermissionResponse},
};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;

use crate::{
    core::{Core, Event, Snapshot},
    protocol::Transport,
    settings::{Settings, Store},
};

const TRAY_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const TRAY_LIVE: &[u8] = include_bytes!("../icons/tray-live.png");
const TRAY_MUTED: &[u8] = include_bytes!("../icons/tray-muted.png");

static QUITTING: AtomicBool = AtomicBool::new(false);

struct Tray {
    icon: TrayIcon,
    status: MenuItem<tauri::Wry>,
    mute: MenuItem<tauri::Wry>,
    disconnect: MenuItem<tauri::Wry>,
    /// 0 idle, 1 live, 2 muted: only swap the icon when it changes.
    shown: Mutex<u8>,
}

fn transport_label(t: Transport) -> &'static str {
    match t {
        Transport::Wifi => "Wi‑Fi",
        Transport::Usb => "cabo USB",
        Transport::Bluetooth => "Bluetooth",
    }
}

fn update_tray(app: &AppHandle, s: &Snapshot) {
    let Some(tray) = app.try_state::<Tray>() else { return };
    let (kind, status, tooltip) = match &s.session {
        Some(sess) if sess.muted => (
            2,
            format!("{} — mutado", sess.device_name),
            format!("MicNexus: {} (mutado)", sess.device_name),
        ),
        Some(sess) => (
            1,
            format!("{} — {}", sess.device_name, transport_label(sess.transport)),
            format!(
                "MicNexus: {} pelo {}",
                sess.device_name,
                transport_label(sess.transport)
            ),
        ),
        None => (
            0,
            "Nenhum celular conectado".to_string(),
            "MicNexus: aguardando o celular".to_string(),
        ),
    };
    let _ = tray.status.set_text(&status);
    let _ = tray.mute.set_text(if kind == 2 {
        "Desmutar microfone"
    } else {
        "Mutar microfone"
    });
    let _ = tray.mute.set_enabled(kind != 0);
    let _ = tray.disconnect.set_enabled(kind != 0);
    let _ = tray.icon.set_tooltip(Some(&tooltip));
    let mut shown = tray.shown.lock().unwrap();
    if *shown != kind {
        *shown = kind;
        let bytes = [TRAY_IDLE, TRAY_LIVE, TRAY_MUTED][kind as usize];
        if let Ok(img) = Image::from_bytes(bytes) {
            let _ = tray.icon.set_icon(Some(img));
        }
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn window_active(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

fn window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn handle_event(app: &AppHandle, event: Event) {
    match event {
        Event::State(s) => {
            update_tray(app, &s);
            let _ = app.emit("state", &*s);
        }
        Event::Level { peak, rms } => {
            if window_visible(app) {
                let _ = app.emit("level", serde_json::json!({ "peak": peak, "rms": rms }));
            }
        }
        Event::Connected { name, transport } => {
            let core = app.state::<Arc<Core>>();
            if core.settings().notify_on_connect && !window_active(app) {
                notify(
                    app,
                    &format!("{name} conectado"),
                    &format!("Microfone pronto pelo {}.", transport_label(transport)),
                );
            }
        }
        Event::Disconnected { name, reason } => {
            let core = app.state::<Arc<Core>>();
            if core.settings().notify_on_connect && !window_active(app) && reason != "user" && reason != "replaced" {
                notify(
                    app,
                    &format!("{name} desconectado"),
                    "O microfone do celular não está mais ativo.",
                );
            }
        }
        Event::Pending { name } => {
            show_main(app);
            notify(
                app,
                "Novo celular",
                &format!("{name} quer usar este PC como microfone. Permita no MicNexus."),
            );
        }
        Event::Declined { .. } => {}
    }
}

fn register_hotkey(app: &AppHandle, hotkey: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let Some(hotkey) = hotkey.filter(|h| !h.trim().is_empty()) else {
        return Ok(());
    };
    gs.on_shortcut(hotkey, |app, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            app.state::<Arc<Core>>().toggle_mute();
        }
    })
    .map_err(|e| e.to_string())
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let autostart = app.autolaunch();
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    if let Err(e) = result {
        eprintln!("autostart: {e}");
    }
}

// ------------------------------------------------------------- commands --

#[tauri::command]
fn get_state(core: tauri::State<Arc<Core>>) -> Snapshot {
    core.snapshot()
}

#[tauri::command]
fn get_settings(core: tauri::State<Arc<Core>>) -> Settings {
    core.settings()
}

/// Merges `patch` (camelCase keys of [`Settings`]) and applies side effects.
#[tauri::command]
fn set_settings(app: AppHandle, core: tauri::State<Arc<Core>>, patch: serde_json::Value) -> Result<Settings, String> {
    let before = core.settings();
    let mut value = serde_json::to_value(&before).map_err(|e| e.to_string())?;
    if let (Some(obj), Some(p)) = (value.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            obj.insert(k.clone(), v.clone());
        }
    }
    let next: Settings = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if next.mute_hotkey != before.mute_hotkey {
        register_hotkey(&app, next.mute_hotkey.as_deref())?;
    }
    if next.start_with_windows != before.start_with_windows {
        apply_autostart(&app, next.start_with_windows);
    }
    let saved = core.update_settings(|s| *s = next.clone());
    if saved.set_default_mic != before.set_default_mic || saved.output_id != before.output_id {
        core.default_mic_setting_changed();
    }
    core.emit_state();
    Ok(saved)
}

#[tauri::command]
fn list_outputs(core: tauri::State<Arc<Core>>) -> Vec<audio::devices::Endpoint> {
    core.refresh_outputs();
    core.outputs()
}

#[tauri::command]
fn set_muted(core: tauri::State<Arc<Core>>, muted: bool) {
    core.set_muted(muted);
}

#[tauri::command]
fn disconnect(core: tauri::State<Arc<Core>>) {
    core.disconnect();
}

#[tauri::command]
fn decide(core: tauri::State<Arc<Core>>, accept: bool) {
    core.decide(accept);
}

#[tauri::command]
fn forget_device(core: tauri::State<Arc<Core>>, id: String) {
    core.forget(&id);
}

#[tauri::command]
fn install_cable(core: tauri::State<Arc<Core>>) {
    cable::install(core.inner().clone());
}

/// Opens the Recording tab of the classic Sound control panel.
#[tauri::command]
fn open_sound_settings() {
    let _ = std::process::Command::new("control.exe").arg("mmsys.cpl,,1").spawn();
}

#[tauri::command]
async fn bluetooth_turn_on() -> Result<(), String> {
    bluetooth::turn_on()
}

#[tauri::command]
fn open_bluetooth_settings() {
    let _ = std::process::Command::new("explorer.exe")
        .arg("ms-settings:bluetooth")
        .spawn();
}

// ------------------------------------------------------------------ run --

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_state,
            get_settings,
            set_settings,
            list_outputs,
            set_muted,
            disconnect,
            decide,
            forget_device,
            install_cable,
            open_sound_settings,
            open_bluetooth_settings,
            bluetooth_turn_on,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = app.path().app_config_dir()?;
            let version = app.package_info().version.to_string();
            let core = Core::new(Store::new(&config_dir), &version, {
                let handle = handle.clone();
                move |event| handle_event(&handle, event)
            });
            app.manage(core.clone());

            let minimized = std::env::args().any(|a| a == "--minimized");
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("MicNexus")
                .inner_size(460.0, 720.0)
                .min_inner_size(400.0, 560.0)
                .visible(!minimized)
                .on_permission_request(|_webview, kind| match kind {
                    PermissionKind::Notifications | PermissionKind::Autoplay => PermissionResponse::Allow,
                    _ => PermissionResponse::Default,
                })
                .build()?;
            let _ = window;

            let status = MenuItem::with_id(app, "status", "Nenhum celular conectado", false, None::<&str>)?;
            let mute = MenuItem::with_id(app, "mute", "Mutar microfone", false, None::<&str>)?;
            let disconnect = MenuItem::with_id(app, "disconnect", "Desconectar", false, None::<&str>)?;
            let open = MenuItem::with_id(app, "open", "Abrir MicNexus", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&status, &sep1, &mute, &disconnect, &sep2, &open, &quit])?;
            let icon = TrayIconBuilder::with_id("main")
                .icon(Image::from_bytes(TRAY_IDLE)?)
                .tooltip("MicNexus")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let core = app.state::<Arc<Core>>();
                    match event.id.as_ref() {
                        "mute" => core.toggle_mute(),
                        "disconnect" => core.disconnect(),
                        "open" => show_main(app),
                        "quit" => {
                            QUITTING.store(true, Ordering::SeqCst);
                            core.disconnect();
                            std::thread::sleep(Duration::from_millis(150));
                            core.restore_default_mic_after_crash();
                            adb::shutdown();
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            app.manage(Tray {
                icon,
                status,
                mute,
                disconnect,
                shown: Mutex::new(0),
            });

            let settings = core.settings();
            if let Err(e) = register_hotkey(&handle, settings.mute_hotkey.as_deref()) {
                eprintln!("hotkey: {e}");
            }
            if settings.start_with_windows && !app.autolaunch().is_enabled().unwrap_or(false) {
                apply_autostart(&handle, true);
            }

            core.refresh_outputs();
            core.restore_default_mic_after_crash();
            lan::start(core.clone());
            bluetooth::start(core.clone());
            adb::start(core.clone(), app.path().resource_dir().ok());
            {
                // Audio endpoints come and go (VB-CABLE installed, headset plugged).
                let core = core.clone();
                std::thread::Builder::new().name("devices".into()).spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(3));
                        core.refresh_outputs();
                    }
                })?;
            }
            update_tray(&handle, &core.snapshot());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps MicNexus in the tray (the phone stays connected).
            if let WindowEvent::CloseRequested { api, .. } = event
                && !QUITTING.load(Ordering::SeqCst)
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building MicNexus")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event
                && code.is_none()
                && !QUITTING.load(Ordering::SeqCst)
            {
                // Last window closed: stay in the tray.
                api.prevent_exit();
                let _ = app;
            }
        });
}
