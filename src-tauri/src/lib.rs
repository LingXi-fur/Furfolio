mod characters;
mod commands;
mod database;
mod error;
mod media;
mod media_decode;
mod media_store;
mod models;

use database::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            let database = Database::initialize(&app_data_dir)?;
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_character,
            commands::list_characters,
            commands::get_character,
            commands::update_character,
            commands::delete_character,
            commands::list_assets,
            commands::import_asset,
            commands::read_asset_preview,
            commands::update_asset,
            commands::delete_asset,
            commands::media_cleanup_status,
            commands::retry_media_cleanup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Furfolio");
}
