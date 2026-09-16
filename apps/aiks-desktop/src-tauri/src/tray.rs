/// System tray setup and menu management.
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开 AIKS", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync_now", "立即同步", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "暂停同步", true, None::<&str>)?;
    let knowledge = MenuItem::with_id(app, "knowledge", "打开知识库", true, None::<&str>)?;
    let data_dir = MenuItem::with_id(app, "data_dir", "打开数据目录", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let startup = MenuItem::with_id(app, "startup", "开机自动启动 ✓", true, None::<&str>)?;
    let separator2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &open,
            &separator,
            &sync,
            &pause,
            &knowledge,
            &data_dir,
            &separator2,
            &startup,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("AIKS — AI 知识同步")
        .on_menu_event(move |app, event| {
            handle_tray_menu(app, event.id.as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            // Double-click to show window
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_main_window(app);
            }
        })
        .build(app)?;

    Ok(())
}

fn handle_tray_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_main_window(app),
        "knowledge" => show_knowledge_window(app),
        "sync_now" => {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app2.try_state::<crate::app_state::AppState>() {
                    if let Some(engine) = state.engine() {
                        let opts = aiks_core::SyncOptions {
                            source_filter: None,
                            dry_run: false,
                            overwrite: false,
                        };
                        // R09: tray sync uses the same orchestration as the UI —
                        // raw sync + pipeline enqueue (extraction submission)
                        let _ = engine.sync_and_enqueue_extraction(opts).await;
                        let _ =
                            app2.emit("sync-complete", serde_json::json!({"triggered_by": "tray"}));
                    }
                }
            });
        }
        "data_dir" => {
            if let Some(state) = app.try_state::<crate::app_state::AppState>() {
                #[cfg(windows)]
                let _ = std::process::Command::new("explorer")
                    .arg(&state.data_dir)
                    .spawn();
                #[cfg(not(windows))]
                let _ = state;
            }
        }
        "quit" => {
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::lifecycle::shutdown(&app2).await;
                app2.exit(0);
            });
        }
        _ => {}
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("control") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn show_knowledge_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("knowledge") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
