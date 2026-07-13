#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cache;
mod config;
mod keychain;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            keychain::get_session_token,
            keychain::set_session_token,
            keychain::delete_session_token,
            cache::get_cached_item,
            cache::set_cached_item,
            cache::clear_cache,
            config::get_gateway_url,
            config::set_gateway_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AETHER Terminal");
}
