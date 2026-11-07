use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterColor {
    pub id: String,
    pub hex_value: String,
    pub position: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterSummary {
    pub id: String,
    pub name: String,
    pub species: Option<String>,
    pub pronouns: Option<String>,
    pub avatar_asset_id: Option<String>,
    pub tags: Vec<Tag>,
    pub colors: Vec<CharacterColor>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Character {
    pub id: String,
    pub name: String,
    pub avatar_asset_id: Option<String>,
    pub species: Option<String>,
    pub pronouns: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<Tag>,
    pub colors: Vec<CharacterColor>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterInput {
    pub name: String,
    #[serde(default)]
    pub species: Option<String>,
    #[serde(default)]
    pub pronouns: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub colors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedCharacterInput {
    pub name: String,
    pub species: Option<String>,
    pub pronouns: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<NormalizedTag>,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedTag {
    pub name: String,
    pub normalized_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub character_id: String,
    pub original_filename: String,
    pub mime_type: String,
    pub byte_size: u64,
    pub width: u32,
    pub height: u32,
    pub artist_name: Option<String>,
    pub source_url: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetInput {
    pub artist_name: Option<String>,
    pub source_url: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportResult {
    pub directory_name: String,
    pub character_count: u64,
    pub asset_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub character_count: u64,
    pub asset_count: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCleanupStatus {
    pub pending: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn media_contract_has_nullable_metadata_without_storage_paths() {
        let input: AssetInput = serde_json::from_value(json!({})).unwrap();
        assert!(input.artist_name.is_none());
        assert!(input.source_url.is_none());
        assert!(input.notes.is_none());
        let asset = Asset {
            id: "a".into(),
            character_id: "c".into(),
            original_filename: "ref.png".into(),
            mime_type: "image/png".into(),
            byte_size: 7,
            width: 2,
            height: 3,
            artist_name: None,
            source_url: None,
            notes: None,
            created_at: "now".into(),
        };
        assert_eq!(
            serde_json::to_value(asset).unwrap(),
            json!({
                "id":"a", "characterId":"c", "originalFilename":"ref.png", "mimeType":"image/png",
                "byteSize":7, "width":2, "height":3, "artistName":null, "sourceUrl":null,
                "notes":null, "createdAt":"now"
            })
        );
    }

    #[test]
    fn serializes_character_contract_in_camel_case() {
        let character = Character {
            id: "character-id".to_owned(),
            name: "Nova".to_owned(),
            avatar_asset_id: Some("asset-id".to_owned()),
            species: Some("Wolf".to_owned()),
            pronouns: None,
            description: None,
            notes: Some("Notes".to_owned()),
            tags: vec![Tag {
                id: "tag-id".to_owned(),
                name: "Main".to_owned(),
            }],
            colors: vec![CharacterColor {
                id: "color-id".to_owned(),
                hex_value: "#123456".to_owned(),
                position: 0,
            }],
            created_at: "2026-01-01T00:00:00Z".to_owned(),
            updated_at: "2026-01-02T00:00:00Z".to_owned(),
        };

        assert_eq!(
            serde_json::to_value(character).expect("serialize character"),
            json!({
                "id": "character-id",
                "name": "Nova",
                "avatarAssetId": "asset-id",
                "species": "Wolf",
                "pronouns": null,
                "description": null,
                "notes": "Notes",
                "tags": [{ "id": "tag-id", "name": "Main" }],
                "colors": [{
                    "id": "color-id",
                    "hexValue": "#123456",
                    "position": 0
                }],
                "createdAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-02T00:00:00Z"
            })
        );
    }
}
