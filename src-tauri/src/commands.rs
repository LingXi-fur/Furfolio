use crate::database::Database;
use crate::error::CommandError;
use crate::models::{
    Asset, AssetInput, Character, CharacterInput, CharacterSummary, MediaCleanupStatus,
};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub fn create_character(
    database: State<'_, Database>,
    input: CharacterInput,
) -> Result<Character, CommandError> {
    crate::characters::create_character(&database, input).map_err(Into::into)
}

#[tauri::command]
pub fn list_characters(
    database: State<'_, Database>,
) -> Result<Vec<CharacterSummary>, CommandError> {
    crate::characters::list_characters(&database).map_err(Into::into)
}

#[tauri::command]
pub fn get_character(database: State<'_, Database>, id: String) -> Result<Character, CommandError> {
    crate::characters::get_character(&database, &id).map_err(Into::into)
}

#[tauri::command]
pub fn update_character(
    database: State<'_, Database>,
    id: String,
    input: CharacterInput,
) -> Result<Character, CommandError> {
    crate::characters::update_character(&database, &id, input).map_err(Into::into)
}

#[tauri::command]
pub async fn delete_character(
    database: State<'_, Database>,
    id: String,
) -> Result<(), CommandError> {
    let database = database.inner().clone();
    worker(move || crate::characters::delete_character(&database, &id)).await
}

async fn worker<T: Send + 'static>(
    task: impl FnOnce() -> Result<T, crate::error::AppError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|_| {
            CommandError::from(crate::error::AppError::Io(std::io::Error::other(
                "Media worker unavailable",
            )))
        })?
        .map_err(Into::into)
}

#[tauri::command]
pub async fn list_assets(
    database: State<'_, Database>,
    character_id: String,
) -> Result<Vec<Asset>, CommandError> {
    let database = database.inner().clone();
    worker(move || crate::media::list_assets(&database, &character_id)).await
}

#[tauri::command]
pub async fn import_asset(
    app: tauri::AppHandle,
    database: State<'_, Database>,
    character_id: String,
) -> Result<Option<Asset>, CommandError> {
    let database = database.inner().clone();
    worker(move || {
        // Only a native dialog supplies a source path; no JS filesystem/dialog capability is granted.
        // The dialog itself is outside the media lock so a cancelled dialog cannot block recovery.
        let selected = app
            .dialog()
            .file()
            .add_filter("Static images", &["png", "jpg", "jpeg", "webp"])
            .blocking_pick_file();
        let selected = selected
            .map(|path| path.into_path())
            .transpose()
            .map_err(|_| crate::error::AppError::InvalidMedia)?;
        crate::media::import_selected(&database, &character_id, selected.as_deref())
    })
    .await
}

#[tauri::command]
pub async fn read_asset_preview(
    database: State<'_, Database>,
    character_id: String,
    asset_id: String,
) -> Result<tauri::ipc::Response, CommandError> {
    let database = database.inner().clone();
    worker(move || {
        crate::media::read_asset_preview(&database, &character_id, &asset_id)
            .map(tauri::ipc::Response::new)
    })
    .await
}

#[tauri::command]
pub async fn update_asset(
    database: State<'_, Database>,
    character_id: String,
    asset_id: String,
    input: AssetInput,
) -> Result<Asset, CommandError> {
    let database = database.inner().clone();
    worker(move || crate::media::update_asset(&database, &character_id, &asset_id, input)).await
}

#[tauri::command]
pub async fn delete_asset(
    database: State<'_, Database>,
    character_id: String,
    asset_id: String,
) -> Result<(), CommandError> {
    let database = database.inner().clone();
    worker(move || crate::media::delete_asset(&database, &character_id, &asset_id)).await
}

#[tauri::command]
pub async fn media_cleanup_status(
    database: State<'_, Database>,
) -> Result<MediaCleanupStatus, CommandError> {
    let database = database.inner().clone();
    worker(move || crate::media::media_cleanup_status(&database)).await
}

#[tauri::command]
pub async fn retry_media_cleanup(
    database: State<'_, Database>,
) -> Result<MediaCleanupStatus, CommandError> {
    let database = database.inner().clone();
    worker(move || crate::media::retry_media_cleanup(&database)).await
}
