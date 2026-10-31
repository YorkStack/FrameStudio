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
            tauri::async_runtime::spawn(transport::worker(worker));
            tauri::async_runtime::spawn(transport::monitor_http(http));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![rpc, choose_directory])
        .run(tauri::generate_context!())?;
    Ok(())
}

// Native menu labels follow the same saved language as the webview.
#[cfg(feature = "desktop")]
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
