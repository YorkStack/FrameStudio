pub mod assistant;
pub mod core;
pub mod exports;
pub mod pricing;
pub mod provider;
pub mod secrets;
pub mod tokens;
pub mod transport;

#[cfg(feature = "desktop")]
#[tauri::command]
async fn rpc(
    app: tauri::AppHandle,
    state: tauri::State<'_, transport::Bridge>,
    op: String,
    args: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let result = state
        .call(&op, args, false)
        .await
        .map_err(|e| e.to_string())?;
    if op == "settings.save" {
        let english = result["language"] == "en";
        native_menu(&app, english).map_err(|e| e.to_string())?;
    }
    Ok(result)
}
#[cfg(feature = "desktop")]
#[tauri::command]
async fn choose_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (send, receive) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = send.send(path.map(|p| p.to_string()));
    });
    receive
        .await
        .map_err(|_| "Folder selection closed".to_string())
}

#[cfg(feature = "desktop")]
pub fn desktop(bridge: transport::Bridge) -> anyhow::Result<()> {
    let settings = tauri::async_runtime::block_on(bridge.core.call(
        "settings.get",
        serde_json::json!({}),
        false,
    ))?;
    let english = settings["language"] == "en";
    let worker = bridge.core.clone();
    let http = bridge.clone();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(bridge)
        .setup(move |app| {
            native_menu(app.handle(), english)?;
            #[cfg(target_os = "windows")]
            fit_windows_start_window(app.handle())?;
            tauri::async_runtime::spawn(transport::worker(worker));
            tauri::async_runtime::spawn(transport::monitor_http(http));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc, choose_directory])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(all(feature = "desktop", target_os = "windows"))]
fn fit_windows_start_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::{Manager, PhysicalPosition, PhysicalSize};
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let Some(monitor) = window.current_monitor()? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let outer = window.outer_size()?;
    let inner = window.inner_size()?;
    let scale = monitor.scale_factor();
    let margin = (8.0 * scale).ceil() as u32;
    let available_width = area
        .size
        .width
        .saturating_sub(outer.width.saturating_sub(inner.width))
        .saturating_sub(margin * 2)
        .max(1);
    let available_height = area
        .size
        .height
        .saturating_sub(outer.height.saturating_sub(inner.height))
        .saturating_sub(margin * 2)
        .max(1);
    window.set_min_size(Some(PhysicalSize::new(
        ((1000.0 * scale) as u32).min(available_width),
        ((650.0 * scale) as u32).min(available_height),
    )))?;
    if outer.width > area.size.width || outer.height > area.size.height {
        window.set_size(PhysicalSize::new(
            inner.width.min(available_width),
            inner.height.min(available_height),
        ))?;
        window.set_position(PhysicalPosition::new(
            area.position.x + margin as i32,
            area.position.y + margin as i32,
        ))?;
    }
    Ok(())
}

// Native menu labels follow the same saved language as the webview.
#[cfg(all(feature = "desktop", target_os = "macos"))]
fn native_menu(app: &tauri::AppHandle, english: bool) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, PredefinedMenuItem as Item, SubmenuBuilder};
    let text = |de, en| if english { en } else { de };
    let application = SubmenuBuilder::new(app, "FrameStudio")
        .item(&Item::about(
            app,
            Some(text("Über FrameStudio", "About FrameStudio")),
            None,
        )?)
        .separator()
        .item(&Item::hide(
            app,
            Some(text("FrameStudio ausblenden", "Hide FrameStudio")),
        )?)
        .item(&Item::hide_others(
            app,
            Some(text("Andere ausblenden", "Hide Others")),
        )?)
        .item(&Item::show_all(
            app,
            Some(text("Alle einblenden", "Show All")),
        )?)
        .separator()
        .item(&Item::quit(
            app,
            Some(text("FrameStudio beenden", "Quit FrameStudio")),
        )?)
        .build()?;
    let edit = SubmenuBuilder::new(app, text("Bearbeiten", "Edit"))
        .item(&Item::undo(app, Some(text("Widerrufen", "Undo")))?)
        .item(&Item::redo(app, Some(text("Wiederholen", "Redo")))?)
        .separator()
        .item(&Item::cut(app, Some(text("Ausschneiden", "Cut")))?)
        .item(&Item::copy(app, Some(text("Kopieren", "Copy")))?)
        .item(&Item::paste(app, Some(text("Einsetzen", "Paste")))?)
        .item(&Item::select_all(
            app,
            Some(text("Alles auswählen", "Select All")),
        )?)
        .build()?;
    let window = SubmenuBuilder::new(app, text("Fenster", "Window"))
        .item(&Item::minimize(app, Some(text("Minimieren", "Minimize")))?)
        .item(&Item::maximize(app, Some(text("Vergrößern", "Zoom")))?)
        .item(&Item::fullscreen(
            app,
            Some(text("Vollbild", "Enter Full Screen")),
        )?)
        .item(&Item::close_window(
            app,
            Some(text("Fenster schließen", "Close Window")),
        )?)
        .build()?;
    app.set_menu(
        MenuBuilder::new(app)
            .items(&[&application, &edit, &window])
            .build()?,
    )?;
    Ok(())
}

// Windows uses only menu commands implemented by its native menu backend.
// In particular, do not offer macOS-only Show All, Hide Others or Full Screen.
#[cfg(all(feature = "desktop", not(target_os = "macos")))]
fn native_menu(app: &tauri::AppHandle, english: bool) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, PredefinedMenuItem as Item, SubmenuBuilder};
    let text = |de, en| if english { en } else { de };
    let application = SubmenuBuilder::new(app, "FrameStudio")
        .item(&Item::about(
            app,
            Some(text("Über FrameStudio", "About FrameStudio")),
            Some(tauri::menu::AboutMetadata {
                name: Some("FrameStudio".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        )?)
        .separator()
        .item(&Item::quit(app, Some(text("Beenden", "Quit")))?)
        .build()?;
    let edit = SubmenuBuilder::new(app, text("Bearbeiten", "Edit"))
        .item(&Item::cut(app, Some(text("Ausschneiden", "Cut")))?)
        .item(&Item::copy(app, Some(text("Kopieren", "Copy")))?)
        .item(&Item::paste(app, Some(text("Einfügen", "Paste")))?)
        .item(&Item::select_all(
            app,
            Some(text("Alles auswählen", "Select All")),
        )?)
        .build()?;
    let window = SubmenuBuilder::new(app, text("Fenster", "Window"))
        .item(&Item::minimize(app, Some(text("Minimieren", "Minimize")))?)
        .item(&Item::maximize(app, Some(text("Maximieren", "Maximize")))?)
        .item(&Item::close_window(
            app,
            Some(text("Fenster schließen", "Close Window")),
        )?)
        .build()?;
    app.set_menu(
        MenuBuilder::new(app)
            .items(&[&application, &edit, &window])
            .build()?,
    )?;
    Ok(())
}
