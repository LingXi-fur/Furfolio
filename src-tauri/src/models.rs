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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
