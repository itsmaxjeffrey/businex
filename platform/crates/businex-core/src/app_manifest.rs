//! Versioned app manifests.
//!
//! A manifest is the complete declarative description of an app: the data
//! entities it stores, the platform permissions it may exercise, its routes,
//! schedules and dependencies. Parsing is strict — unknown fields and unknown
//! permissions are errors, never silently ignored — so a malformed manifest
//! can never reach the record layer or the runtime.

use crate::{Error, Permission, Result, Role};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Manifest schema revision understood by this platform version.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    /// Data-driven app: entities and screens generated from the manifest.
    Schema,
    /// Custom TypeScript app executing in an isolated container.
    Code,
}

impl AppKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AppKind::Schema => "schema",
            AppKind::Code => "code",
        }
    }

    pub fn parse(s: &str) -> Option<AppKind> {
        match s {
            "schema" => Some(AppKind::Schema),
            "code" => Some(AppKind::Code),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Text,
    Number,
    Bool,
    Date,
}

impl FieldType {
    pub fn as_str(self) -> &'static str {
        match self {
            FieldType::Text => "text",
            FieldType::Number => "number",
            FieldType::Bool => "bool",
            FieldType::Date => "date",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub unique: bool,
}

/// Minimum company role required for each record action on one entity.
///
/// Entity rules only ever narrow the company-wide permission grants — the
/// permission check still runs first — so a manifest can keep warehouse
/// members on `item` records while `stocktake` writes stay manager-only
/// without granting anyone anything new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityAccess {
    /// Role required to read records. Default: viewer.
    #[serde(default = "default_read_role")]
    pub read: Role,
    /// Role required to create or update records. Default: member.
    #[serde(default = "default_write_role")]
    pub write: Role,
    /// Role required to delete records. Default: manager.
    #[serde(default = "default_delete_role")]
    pub delete: Role,
}

fn default_read_role() -> Role {
    Role::Viewer
}

fn default_write_role() -> Role {
    Role::Member
}

fn default_delete_role() -> Role {
    Role::Manager
}

impl Default for EntityAccess {
    fn default() -> Self {
        // The defaults mirror the plain permission mapping exactly, so a
        // manifest that omits `access` behaves as it always has.
        EntityAccess {
            read: Role::Viewer,
            write: Role::Member,
            delete: Role::Manager,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityDef {
    pub name: String,
    pub fields: Vec<FieldDef>,
    /// Per-action role floor for this entity's records. An omitted action
    /// falls back to the platform default above.
    #[serde(default)]
    pub access: EntityAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteDef {
    pub path: String,
    pub method: HttpMethod,
    /// Permission this route exercises. Deserialization is strict: an unknown
    /// permission string fails the whole manifest.
    pub permission: Permission,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleDef {
    pub name: String,
    /// Cron expression; whitespace-separated with at least five fields.
    pub cron: String,
    pub handler: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppManifest {
    pub schema_version: u32,
    pub name: String,
    pub slug: String,
    pub kind: AppKind,
    #[serde(default)]
    pub entities: Vec<EntityDef>,
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default)]
    pub routes: Vec<RouteDef>,
    #[serde(default)]
    pub schedules: Vec<ScheduleDef>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn valid_ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Schedule handlers name exported functions in generated code, so camelCase
/// identifiers are legitimate here (unlike entity and field names).
fn valid_handler(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && s.len() <= 64
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl AppManifest {
    pub fn entity(&self, name: &str) -> Option<&EntityDef> {
        self.entities.iter().find(|e| e.name == name)
    }

    /// Strict structural validation. Everything checked here is also enforced
    /// at the storage layer; failing early keeps bad manifests out entirely.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(Error::Invalid {
                message: format!(
                    "unsupported manifest schema_version: {}",
                    self.schema_version
                ),
            });
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 120 {
            return Err(Error::Invalid {
                message: "manifest name must be 1-120 characters".into(),
            });
        }
        if !valid_slug(&self.slug) {
            return Err(Error::Invalid {
                message: format!("invalid app slug: {}", self.slug),
            });
        }
        if self.kind == AppKind::Schema && self.entities.is_empty() {
            return Err(Error::Invalid {
                message: "schema apps must declare at least one entity".into(),
            });
        }

        let mut entities = BTreeSet::new();
        for entity in &self.entities {
            if !valid_ident(&entity.name) {
                return Err(Error::Invalid {
                    message: format!("invalid entity name: {}", entity.name),
                });
            }
            if !entities.insert(entity.name.as_str()) {
                return Err(Error::Invalid {
                    message: format!("duplicate entity: {}", entity.name),
                });
            }
            if entity.fields.is_empty() {
                return Err(Error::Invalid {
                    message: format!("entity {} must declare at least one field", entity.name),
                });
            }
            let mut fields = BTreeSet::new();
            for field in &entity.fields {
                if !valid_ident(&field.name) {
                    return Err(Error::Invalid {
                        message: format!("invalid field name: {}", field.name),
                    });
                }
                if !fields.insert(field.name.as_str()) {
                    return Err(Error::Invalid {
                        message: format!("duplicate field: {}.{}", entity.name, field.name),
                    });
                }
            }
        }

        for route in &self.routes {
            if !route.path.starts_with('/') {
                return Err(Error::Invalid {
                    message: format!("route path must start with /: {}", route.path),
                });
            }
        }

        let mut schedules = BTreeSet::new();
        for schedule in &self.schedules {
            if !valid_ident(&schedule.name) {
                return Err(Error::Invalid {
                    message: format!("invalid schedule name: {}", schedule.name),
                });
            }
            if !schedules.insert(schedule.name.as_str()) {
                return Err(Error::Invalid {
                    message: format!("duplicate schedule: {}", schedule.name),
                });
            }
            if schedule.cron.split_whitespace().count() < 5 {
                return Err(Error::Invalid {
                    message: format!("invalid cron expression for schedule: {}", schedule.name),
                });
            }
            if !valid_handler(&schedule.handler) {
                return Err(Error::Invalid {
                    message: format!("invalid schedule handler: {}", schedule.handler),
                });
            }
        }

        let mut dependencies = BTreeSet::new();
        for dep in &self.dependencies {
            if !valid_slug(dep) {
                return Err(Error::Invalid {
                    message: format!("invalid dependency slug: {}", dep),
                });
            }
            if *dep == self.slug {
                return Err(Error::Invalid {
                    message: "app cannot depend on itself".into(),
                });
            }
            if !dependencies.insert(dep.as_str()) {
                return Err(Error::Invalid {
                    message: format!("duplicate dependency: {}", dep),
                });
            }
        }

        Ok(())
    }
}

/// Validate one record document against its entity schema: no unknown keys,
/// every required field present, every value of the declared type.
pub fn validate_record(entity: &EntityDef, data: &serde_json::Value) -> Result<()> {
    let obj = data.as_object().ok_or_else(|| Error::Invalid {
        message: "record data must be a JSON object".into(),
    })?;
    for key in obj.keys() {
        if !entity.fields.iter().any(|f| f.name == *key) {
            return Err(Error::Invalid {
                message: format!("unknown field for entity {}: {}", entity.name, key),
            });
        }
    }
    for field in &entity.fields {
        match obj.get(&field.name) {
            None => {
                if field.required {
                    return Err(Error::Invalid {
                        message: format!("missing required field: {}", field.name),
                    });
                }
            }
            Some(value) => {
                let ok = match field.field_type {
                    FieldType::Text => value.is_string(),
                    FieldType::Number => value.is_number(),
                    FieldType::Bool => value.is_boolean(),
                    FieldType::Date => value
                        .as_str()
                        .map(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok())
                        .unwrap_or(false),
                };
                if !ok {
                    return Err(Error::Invalid {
                        message: format!(
                            "field {} must be of type {}",
                            field.name,
                            field.field_type.as_str()
                        ),
                    });
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json(extra_entity_fields: &str) -> String {
        format!(
            r#"{{"schema_version":1,"name":"Inventory","slug":"inventory","kind":"schema",
                 "entities":[{{"name":"item","fields":[{{"name":"sku","type":"text","required":true,"unique":true{}}}]}}],
                 "permissions":["records.read","records.write"],
                 "routes":[{{"path":"/items","method":"get","permission":"records.read"}}],
                 "schedules":[{{"name":"nightly","cron":"0 3 * * *","handler":"refreshStock"}}],
                 "dependencies":[]}}"#,
            extra_entity_fields
        )
    }

    #[test]
    fn valid_manifest_parses_and_validates() {
        let m: AppManifest =
            serde_json::from_str(&manifest_json("")).expect("valid manifest must parse");
        m.validate().expect("valid manifest must validate");
    }

    #[test]
    fn unknown_permission_is_rejected_at_parse() {
        let raw = manifest_json("").replace("records.write", "records.destroy");
        assert!(serde_json::from_str::<AppManifest>(&raw).is_err());
    }

    #[test]
    fn unknown_manifest_field_is_rejected() {
        let raw =
            manifest_json("").replace(r#""dependencies":[]"#, r#""dependencies":[],"bogus":1"#);
        assert!(serde_json::from_str::<AppManifest>(&raw).is_err());
    }

    #[test]
    fn bad_slug_and_duplicate_entities_are_rejected() {
        let mut m: AppManifest = serde_json::from_str(&manifest_json("")).expect("parse");
        m.slug = "Bad_Slug".into();
        assert!(m.validate().is_err());
        m.slug = "inventory".into();
        m.entities.push(m.entities[0].clone());
        assert!(m.validate().is_err());
    }

    #[test]
    fn schema_app_needs_entities_and_self_dependency_is_rejected() {
        let mut m: AppManifest = serde_json::from_str(&manifest_json("")).expect("parse");
        m.entities.clear();
        assert!(m.validate().is_err());
        let mut m: AppManifest = serde_json::from_str(&manifest_json("")).expect("parse");
        m.dependencies.push("inventory".into());
        assert!(m.validate().is_err());
    }

    #[test]
    fn record_validation_enforces_schema() {
        let m: AppManifest = serde_json::from_str(&manifest_json("")).expect("parse");
        let entity = m.entity("item").expect("entity present");
        assert!(validate_record(entity, &serde_json::json!({"sku": "A-1"})).is_ok());
        assert!(validate_record(entity, &serde_json::json!({})).is_err());
        assert!(validate_record(entity, &serde_json::json!({"sku": "A-1", "x": 1})).is_err());
        assert!(validate_record(entity, &serde_json::json!({"sku": 7})).is_err());
    }
}
