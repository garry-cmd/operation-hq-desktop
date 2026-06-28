use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_dialog::DialogExt;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // ── Custom URI scheme: hq://localhost/<command> ──────────────────
        // Called via fetch('hq://localhost/pick-file') from the webview.
        // Bypasses the IPC/ACL system entirely — no capability config needed.
        .register_asynchronous_uri_scheme_protocol("hq", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();

            std::thread::spawn(move || {
                fn json_response(body: String) -> http::Response<Vec<u8>> {
                    http::Response::builder()
                        .header("Content-Type", "application/json")
                        .header("Access-Control-Allow-Origin", "https://hq.svirene.com")
                        .body(body.into_bytes())
                        .unwrap()
                }

                fn error_response(msg: &str) -> http::Response<Vec<u8>> {
                    http::Response::builder()
                        .status(400)
                        .header("Content-Type", "application/json")
                        .header("Access-Control-Allow-Origin", "https://hq.svirene.com")
                        .body(format!("{{\"error\":\"{}\"}}", msg).into_bytes())
                        .unwrap()
                }

                match path.as_str() {
                    "/ping" => {
                        responder.respond(json_response("{\"ok\":true}".to_string()));
                    }

                    "/pick-file" => {
                        let result = app.dialog().file().blocking_pick_file();
                        let body = match result {
                            Some(p) => format!("{{\"path\":{}}}", 
                                serde_json::to_string(&p.to_string()).unwrap_or("null".to_string())),
                            None => "{\"path\":null}".to_string(),
                        };
                        responder.respond(json_response(body));
                    }

                    "/pick-folder" => {
                        let result = app.dialog().file().blocking_pick_folder();
                        let body = match result {
                            Some(p) => format!("{{\"path\":{}}}", 
                                serde_json::to_string(&p.to_string()).unwrap_or("null".to_string())),
                            None => "{\"path\":null}".to_string(),
                        };
                        responder.respond(json_response(body));
                    }

                    "/shell-open" => {
                        // URL is passed as a query param: hq://localhost/shell-open?url=...
                        let uri = request.uri();
                        let query = uri.query().unwrap_or("");
                        let url = url_decode(
                            query.split('&')
                                .find(|p| p.starts_with("url="))
                                .and_then(|p| p.strip_prefix("url="))
                                .unwrap_or("")
                        );
                        if url.is_empty() {
                            responder.respond(error_response("missing url param"));
                        } else {
                            let _ = open::that(&url);
                            responder.respond(json_response("{\"ok\":true}".to_string()));
                        }
                    }

                    _ => {
                        responder.respond(error_response("unknown command"));
                    }
                }
            });
        })
        .setup(|app| {
            // ── Tray icon ──────────────────────────────────────────────
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

            // ── Global shortcuts ────────────────────────────────────────
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

/// Percent-decode a URL-encoded string.
fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(0);
            let h2 = chars.next().unwrap_or(0);
            if let Ok(decoded) = u8::from_str_radix(
                std::str::from_utf8(&[h1, h2]).unwrap_or(""),
                16,
            ) {
                result.push(decoded as char);
            }
        } else if b == b'+' {
            result.push(' ');
        } else {
            result.push(b as char);
        }
    }
    result
}
