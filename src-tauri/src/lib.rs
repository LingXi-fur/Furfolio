mod characters;
mod commands;
mod database;
mod error;
mod models;

use database::Database;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Furfolio");
}
