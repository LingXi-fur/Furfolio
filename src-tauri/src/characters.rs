use crate::database::Database;
use crate::error::AppError;
use crate::models::{
    Character, CharacterColor, CharacterInput, CharacterSummary, NormalizedCharacterInput,
    NormalizedTag, Tag,
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

pub fn create_character(database: &Database, input: CharacterInput) -> Result<Character, AppError> {
    let input = normalize_input(input)?;
    let mut connection = database.open()?;
    let transaction = connection.transaction()?;
    let id = Uuid::new_v4().to_string();
    let timestamp = current_timestamp();

    transaction.execute(
        "INSERT INTO characters (
            id, name, species, pronouns, description, notes, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            input.name,
            input.species,
            input.pronouns,
            input.description,
            input.notes,
            timestamp,
        ],
    )?;
    replace_tags(&transaction, &id, &input.tags)?;
    replace_colors(&transaction, &id, &input.colors)?;
    transaction.commit()?;

    get_character(database, &id)
}

pub fn list_characters(database: &Database) -> Result<Vec<CharacterSummary>, AppError> {
    let mut connection = database.open()?;
    let transaction = connection.transaction()?;
    let mut characters = {
        let mut statement = transaction.prepare(
            "SELECT id, name, species, pronouns, avatar_asset_id, updated_at
             FROM characters
             ORDER BY name COLLATE NOCASE, id",
        )?;
        let characters = statement
            .query_map([], |row| {
                Ok(CharacterSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    species: row.get(2)?,
                    pronouns: row.get(3)?,
                    avatar_asset_id: row.get(4)?,
                    tags: Vec::new(),
                    colors: Vec::new(),
                    updated_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        characters
    };
    for character in &mut characters {
        character.tags = load_tags(&transaction, &character.id)?;
        character.colors = load_colors(&transaction, &character.id)?;
    }
    transaction.commit()?;
    Ok(characters)
}

pub fn get_character(database: &Database, id: &str) -> Result<Character, AppError> {
    let mut connection = database.open()?;
    let transaction = connection.transaction()?;
    let character = load_character(&transaction, id)?;
    transaction.commit()?;
    Ok(character)
}

pub fn update_character(
    database: &Database,
    id: &str,
    input: CharacterInput,
) -> Result<Character, AppError> {
    let input = normalize_input(input)?;
    let mut connection = database.open()?;
    let transaction = connection.transaction()?;
    let timestamp = current_timestamp();
    let changed = transaction.execute(
        "UPDATE characters
         SET name = ?1, species = ?2, pronouns = ?3, description = ?4, notes = ?5,
             updated_at = ?6
         WHERE id = ?7",
        params![
            input.name,
            input.species,
            input.pronouns,
            input.description,
            input.notes,
            timestamp,
            id,
        ],
    )?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }

    replace_tags(&transaction, id, &input.tags)?;
    replace_colors(&transaction, id, &input.colors)?;
    remove_orphaned_tags(&transaction)?;
    transaction.commit()?;

    get_character(database, id)
}

pub fn delete_character(database: &Database, id: &str) -> Result<(), AppError> {
    let mut connection = database.open()?;
    let transaction = connection.transaction()?;
    let changed = transaction.execute("DELETE FROM characters WHERE id = ?1", params![id])?;
    if changed == 0 {
        return Err(AppError::NotFound);
    }
    remove_orphaned_tags(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn normalize_input(input: CharacterInput) -> Result<NormalizedCharacterInput, AppError> {
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::InvalidInput("Character name is required"));
    }

    let mut seen_tags = HashSet::new();
    let mut tags = Vec::new();
    for value in input.tags {
        let name = value.trim().to_owned();
        if name.is_empty() {
            continue;
        }
        let normalized_name = name.to_lowercase();
        if seen_tags.insert(normalized_name.clone()) {
            tags.push(NormalizedTag {
                name,
                normalized_name,
            });
        }
    }

    let mut colors = Vec::with_capacity(input.colors.len());
    for value in input.colors {
        let value = value.trim();
        if !is_hex_color(value) {
            return Err(AppError::InvalidInput("Colors must use the #RRGGBB format"));
        }
        colors.push(value.to_ascii_uppercase());
    }

    Ok(NormalizedCharacterInput {
        name,
        species: normalize_optional(input.species),
        pronouns: normalize_optional(input.pronouns),
        description: normalize_optional(input.description),
        notes: normalize_optional(input.notes),
        tags,
        colors,
    })
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn current_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true)
}

fn replace_tags(
    transaction: &Transaction<'_>,
    character_id: &str,
    tags: &[NormalizedTag],
) -> Result<(), AppError> {
    transaction.execute(
        "DELETE FROM character_tags WHERE character_id = ?1",
        params![character_id],
    )?;

    for (position, tag) in tags.iter().enumerate() {
        transaction.execute(
            "INSERT INTO tags (id, name, normalized_name)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(normalized_name) DO NOTHING",
            params![Uuid::new_v4().to_string(), tag.name, tag.normalized_name],
        )?;
        let tag_id: String = transaction.query_row(
            "SELECT id FROM tags WHERE normalized_name = ?1",
            params![tag.normalized_name],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO character_tags (character_id, tag_id, position)
             VALUES (?1, ?2, ?3)",
            params![character_id, tag_id, position as i64],
        )?;
    }
    Ok(())
}

fn replace_colors(
    transaction: &Transaction<'_>,
    character_id: &str,
    colors: &[String],
) -> Result<(), AppError> {
    transaction.execute(
        "DELETE FROM character_colors WHERE character_id = ?1",
        params![character_id],
    )?;
    for (position, color) in colors.iter().enumerate() {
        transaction.execute(
            "INSERT INTO character_colors (id, character_id, hex_value, position)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                Uuid::new_v4().to_string(),
                character_id,
                color,
                position as i64,
            ],
        )?;
    }
    Ok(())
}

fn remove_orphaned_tags(transaction: &Transaction<'_>) -> Result<(), AppError> {
    transaction.execute(
        "DELETE FROM tags
         WHERE NOT EXISTS (
             SELECT 1 FROM character_tags WHERE character_tags.tag_id = tags.id
         )",
        [],
    )?;
    Ok(())
}

fn load_character(connection: &Connection, id: &str) -> Result<Character, AppError> {
    let character = connection
        .query_row(
            "SELECT id, name, avatar_asset_id, species, pronouns, description, notes,
                    created_at, updated_at
             FROM characters
             WHERE id = ?1",
            params![id],
            |row| {
                Ok(Character {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    avatar_asset_id: row.get(2)?,
                    species: row.get(3)?,
                    pronouns: row.get(4)?,
                    description: row.get(5)?,
                    notes: row.get(6)?,
                    tags: Vec::new(),
                    colors: Vec::new(),
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()?
        .ok_or(AppError::NotFound)?;

    let mut character = character;
    character.tags = load_tags(connection, id)?;
    character.colors = load_colors(connection, id)?;
    Ok(character)
}

fn load_tags(connection: &Connection, character_id: &str) -> Result<Vec<Tag>, AppError> {
    let mut statement = connection.prepare(
        "SELECT tags.id, tags.name
         FROM tags
         JOIN character_tags ON character_tags.tag_id = tags.id
         WHERE character_tags.character_id = ?1
         ORDER BY character_tags.position",
    )?;
    let tags = statement
        .query_map(params![character_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tags)
}

fn load_colors(
    connection: &Connection,
    character_id: &str,
) -> Result<Vec<CharacterColor>, AppError> {
    let mut statement = connection.prepare(
        "SELECT id, hex_value, position
         FROM character_colors
         WHERE character_id = ?1
         ORDER BY position",
    )?;
    let colors = statement
        .query_map(params![character_id], |row| {
            Ok(CharacterColor {
                id: row.get(0)?,
                hex_value: row.get(1)?,
                position: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(colors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn temporary_database() -> (Database, PathBuf) {
        let path = std::env::temp_dir().join(format!("furfolio-{}.sqlite3", Uuid::new_v4()));
        let database = Database::initialize_at(path.clone()).expect("initialize database");
        (database, path)
    }

    fn remove_database(path: &Path) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(path.with_extension("sqlite3-shm"));
        let _ = fs::remove_file(path.with_extension("sqlite3-wal"));
    }

    fn character_input(name: &str) -> CharacterInput {
        CharacterInput {
            name: name.to_owned(),
            species: Some("  Wolf  ".to_owned()),
            pronouns: Some("they/them".to_owned()),
            description: None,
            notes: Some("  ".to_owned()),
            tags: vec![" Main ".to_owned(), "main".to_owned(), "Blue".to_owned()],
            colors: vec!["#12abEF".to_owned(), "#FFFFFF".to_owned()],
        }
    }

    #[test]
    fn creates_lists_and_reopens_characters() {
        let (database, path) = temporary_database();
        let character =
            create_character(&database, character_input("  Nova  ")).expect("create character");

        assert_eq!(character.name, "Nova");
        assert_eq!(character.species.as_deref(), Some("Wolf"));
        assert_eq!(character.notes, None);
        assert_eq!(character.tags.len(), 2);
        assert_eq!(character.tags[0].name, "Main");
        assert_eq!(character.colors[0].hex_value, "#12ABEF");

        let reopened = Database::initialize_at(path.clone()).expect("reopen database");
        let saved = get_character(&reopened, &character.id).expect("load character");
        assert_eq!(saved.name, "Nova");
        let summaries = list_characters(&reopened).expect("list characters");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].tags[0].name, "Main");
        assert_eq!(summaries[0].colors[0].hex_value, "#12ABEF");

        remove_database(&path);
    }

    #[test]
    fn rejects_invalid_boundary_values() {
        let (database, path) = temporary_database();
        let empty_name = CharacterInput {
            name: "   ".to_owned(),
            ..character_input("unused")
        };
        assert!(matches!(
            create_character(&database, empty_name),
            Err(AppError::InvalidInput(_))
        ));

        let invalid_color = CharacterInput {
            colors: vec!["red".to_owned()],
            ..character_input("Nova")
        };
        assert!(matches!(
            create_character(&database, invalid_color),
            Err(AppError::InvalidInput(_))
        ));
        assert!(list_characters(&database)
            .expect("list characters")
            .is_empty());

        remove_database(&path);
    }

    #[test]
    fn updates_associations_and_reuses_normalized_tags() {
        let (database, path) = temporary_database();
        let first = create_character(&database, character_input("Nova")).expect("create first");
        let second = create_character(
            &database,
            CharacterInput {
                tags: vec!["MAIN".to_owned()],
                colors: Vec::new(),
                ..character_input("Echo")
            },
        )
        .expect("create second");
        assert_eq!(first.tags[0].id, second.tags[0].id);

        let updated = update_character(
            &database,
            &first.id,
            CharacterInput {
                species: None,
                tags: vec!["Solo".to_owned()],
                colors: vec!["#000000".to_owned()],
                ..character_input("Nova Prime")
            },
        )
        .expect("update character");
        assert_eq!(updated.name, "Nova Prime");
        assert_eq!(updated.species, None);
        assert_eq!(updated.tags[0].name, "Solo");
        assert_eq!(updated.colors.len(), 1);
        assert_ne!(updated.updated_at, updated.created_at);

        remove_database(&path);
    }

    #[test]
    fn cascades_character_relations_and_cleans_orphaned_tags() {
        let (database, path) = temporary_database();
        let character = create_character(&database, character_input("Nova")).expect("create");
        delete_character(&database, &character.id).expect("delete");

        let connection = database.open().expect("open database");
        for table in ["characters", "character_tags", "character_colors", "tags"] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("count rows");
            assert_eq!(count, 0, "{table} should be empty");
        }

        remove_database(&path);
    }

    #[test]
    fn rolls_back_a_failed_update() {
        let (database, path) = temporary_database();
        let character = create_character(&database, character_input("Nova")).expect("create");
        let connection = database.open().expect("open database");
        connection
            .execute_batch(
                "CREATE TRIGGER reject_colors
                 BEFORE INSERT ON character_colors
                 BEGIN
                     SELECT RAISE(ABORT, 'rejected by test');
                 END;",
            )
            .expect("create trigger");
        drop(connection);

        let result = update_character(
            &database,
            &character.id,
            CharacterInput {
                colors: vec!["#000000".to_owned()],
                ..character_input("Changed")
            },
        );
        assert!(matches!(result, Err(AppError::Database(_))));

        let saved = get_character(&database, &character.id).expect("reload character");
        assert_eq!(saved.name, "Nova");
        assert_eq!(saved.colors.len(), 2);
        assert_eq!(saved.tags.len(), 2);

        remove_database(&path);
    }
}
