use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.unminimize();
    }
}

fn emit_capture<R: Runtime>(app: &AppHandle<R>, kind: &str) {
    show_window(app);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("hq:capture", kind);
    }
}

// ── Invoke commands — only called from tauri://localhost (trusted) ─────────

#[tauri::command]
async fn pick_file(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().blocking_pick_file();
    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().blocking_pick_folder();
    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
async fn shell_open(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
async fn ping() -> Result<bool, String> {
    Ok(true)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![ping, pick_file, pick_folder, shell_open])
        .setup(|app| {
            // ── Tray icon ─────────────────────────────────────────────
            let open_item = MenuItem::with_id(app, "open", "Open Operation HQ", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &quit_item])?;

            let icon = Image::from_path("icons/32x32.png").unwrap_or_else(|_| {
                app.default_window_icon().cloned().unwrap_or_else(|| {
                    Image::from_bytes(&[0u8; 4]).unwrap()
                })
            });

            TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // ── Global shortcuts ──────────────────────────────────────
            let shortcut_new_task = Shortcut::new(Some(Modifiers::META), Code::KeyT);
            let shortcut_new_note = Shortcut::new(Some(Modifiers::META), Code::KeyN);

            app.global_shortcut().on_shortcuts(
                [shortcut_new_task, shortcut_new_note],
                move |app, shortcut, _event| {
                    let kind = if shortcut.key == Code::KeyT { "task" } else { "note" };
                    emit_capture(app, kind);
                },
            )?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                #[cfg(target_os = "macos")]
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
