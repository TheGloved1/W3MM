// TODO: replace the example command with your own. Add new commands to the
// invoke_handler! list in run().
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // --- UPDATER (optional) ---
        // To enable in-app updates later, add the updater plugin here:
        //   .plugin(tauri_plugin_updater::Builder::new().build())
        // (also uncomment the dependency in Cargo.toml — marked UPDATER)
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
