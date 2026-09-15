use crate::database::Database;
use crate::error::CommandError;
use crate::models::{Character, CharacterInput, CharacterSummary};
use tauri::State;

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
pub fn delete_character(database: State<'_, Database>, id: String) -> Result<(), CommandError> {
    crate::characters::delete_character(&database, &id).map_err(Into::into)
}
