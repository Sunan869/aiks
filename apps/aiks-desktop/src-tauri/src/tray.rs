/// System tray setup and menu management.
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

pub fn setup_tray(app: &tauri::App) -> anyhow::Result<()> {
    let service_mode = crate::lifecycle::selected_config()
        .map(|c| c.backend.mode == aiks_core::config::BackendMode::ServiceLocal)
        .unwrap_or(false);
    let open = MenuItem::with_id(app, "open", "打开 AIKS", true, None::<&str>)?;
    let sync = MenuItem::with_id(
        app,
        "sync_now",
        if service_mode {
            "选择来源采集"
        } else {
            "立即同步"
        },
        true,
        None::<&str>,
    )?;
    let pause = MenuItem::with_id(app, "pause", "暂停同步", !service_mode, None::<&str>)?;
    let knowledge = MenuItem::with_id(app, "knowledge", "打开知识库", true, None::<&str>)?;
    let data_dir = MenuItem::with_id(app, "data_dir", "打开数据目录", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let startup = MenuItem::with_id(
        app,
        "startup",
        "开机自动启动 ✓",
        !service_mode,
        None::<&str>,
    )?;
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
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}
fn handle_tray_menu(app: &AppHandle, id: &str) {
    let service_mode = app
        .try_state::<std::sync::Arc<crate::service_desktop::ServiceDesktop>>()
        .is_some();
    if service_mode {
        match id {
            "open" | "knowledge" | "sync_now" | "pause" => {
                show_main_window(app);
                return;
            }
            "data_dir" => {
                let root = crate::app_state::data_dir().join("service-local");
                #[cfg(windows)]
                let program = "explorer";
                #[cfg(target_os = "macos")]
                let program = "open";
                #[cfg(not(any(windows, target_os = "macos")))]
                let program = "xdg-open";
                let _ = std::process::Command::new(program).arg(root).spawn();
                return;
            }
            _ => {}
        }
    }
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
