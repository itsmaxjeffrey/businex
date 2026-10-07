//! Domain primitives shared across the Businex platform.
//!
//! This crate is intentionally free of I/O: roles, permissions, subjects and
//! errors are pure data so the API layer, the workers and the tests all apply
//! exactly the same authorization rules.
//!
//! Canonical permission representation: dotted lowercase strings (for example
//! "records.write") everywhere: manifests, SDK, API and logs. Parsing is
//! strict: an unknown permission is an error, never silently ignored.

use chrono::{DateTime, Utc};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
use std::fmt;

/// Company membership roles, ordered from least to most privileged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Member,
    Manager,
    Admin,
    Owner,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::Viewer,
        Role::Member,
        Role::Manager,
        Role::Admin,
        Role::Owner,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Member => "member",
            Role::Manager => "manager",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "viewer" => Some(Role::Viewer),
            "member" => Some(Role::Member),
            "manager" => Some(Role::Manager),
            "admin" => Some(Role::Admin),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }

    /// Fixed permission set for the role. Owner holds every permission.
    pub fn permissions(self) -> BTreeSet<Permission> {
        use Permission::*;
        let mut p = BTreeSet::new();
        p.insert(RecordsRead);
        if self >= Role::Member {
            p.insert(RecordsWrite);
            p.insert(FilesWrite);
            p.insert(CommunicationWrite);
            p.insert(AppsUse);
        }
        if self >= Role::Manager {
            p.insert(RecordsDelete);
            p.insert(TasksManage);
            p.insert(AgentsRun);
            p.insert(JobsManage);
        }
        if self >= Role::Admin {
            p.insert(MembersManage);
            p.insert(SettingsManage);
            p.insert(AppsInstall);
            p.insert(AppsManage);
            p.insert(AgentsManage);
            p.insert(AgentsApprove);
            p.insert(AuditRead);
            p.insert(ModelKeysManage);
        }
        if self == Role::Owner {
            p.insert(CompanyTransfer);
            p.insert(CompanyDelete);
        }
        p
    }

    pub fn allows(self, action: Permission) -> bool {
        self.permissions().contains(&action)
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Scoped action permissions checked for humans, agents and generated apps.
/// Serialized as the canonical dotted string form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Permission {
    RecordsRead,
    RecordsWrite,
    RecordsDelete,
    FilesWrite,
    CommunicationWrite,
    AppsUse,
    AppsInstall,
    AppsManage,
    TasksManage,
    AgentsRun,
    AgentsManage,
    AgentsApprove,
    JobsManage,
    MembersManage,
    SettingsManage,
    AuditRead,
    ModelKeysManage,
    CompanyTransfer,
    CompanyDelete,
}

impl Permission {
    pub const ALL: [Permission; 19] = [
        Permission::RecordsRead,
        Permission::RecordsWrite,
        Permission::RecordsDelete,
        Permission::FilesWrite,
        Permission::CommunicationWrite,
        Permission::AppsUse,
        Permission::AppsInstall,
        Permission::AppsManage,
        Permission::TasksManage,
        Permission::AgentsRun,
        Permission::AgentsManage,
        Permission::AgentsApprove,
        Permission::JobsManage,
        Permission::MembersManage,
        Permission::SettingsManage,
        Permission::AuditRead,
        Permission::ModelKeysManage,
        Permission::CompanyTransfer,
        Permission::CompanyDelete,
    ];

    /// Canonical dotted form used by manifests, SDK, API and logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Permission::RecordsRead => "records.read",
            Permission::RecordsWrite => "records.write",
            Permission::RecordsDelete => "records.delete",
            Permission::FilesWrite => "files.write",
            Permission::CommunicationWrite => "communication.write",
            Permission::AppsUse => "apps.use",
            Permission::AppsInstall => "apps.install",
            Permission::AppsManage => "apps.manage",
            Permission::TasksManage => "tasks.manage",
            Permission::AgentsRun => "agents.run",
            Permission::AgentsManage => "agents.manage",
            Permission::AgentsApprove => "agents.approve",
            Permission::JobsManage => "jobs.manage",
            Permission::MembersManage => "members.manage",
            Permission::SettingsManage => "settings.manage",
            Permission::AuditRead => "audit.read",
            Permission::ModelKeysManage => "model_keys.manage",
            Permission::CompanyTransfer => "company.transfer",
            Permission::CompanyDelete => "company.delete",
        }
    }

    /// Strict parse: unknown permissions are an error, never a fallback.
    pub fn parse(s: &str) -> std::result::Result<Permission, Error> {
        for p in Permission::ALL {
            if p.as_str() == s {
                return Ok(p);
            }
        }
        Err(Error::Invalid {
            message: format!("unknown permission: {}", s),
        })
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Permission {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D: Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Permission, D::Error> {
        let s = String::deserialize(deserializer)?;
        Permission::parse(&s).map_err(|e| D::Error::custom(e.to_string()))
    }
}

/// Who is acting. The actor identity and the company id are established by
/// the server from a verified session, membership lookup or service
/// credential. They must never be accepted from request bodies or generated
/// app input; System is constructed by server-side code only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Actor {
    User { id: uuid::Uuid },
    Agent { id: uuid::Uuid },
    GeneratedApp { app_id: uuid::Uuid, install_id: uuid::Uuid },
    System,
}

/// Resource scope for scoped action grants (agents, generated apps).
///
/// Unrestricted access is the explicit variant All and exists only after an
/// authorized grant decision (a verified human membership). Scoped
/// credentials are Restricted: an empty or malformed grant list means deny
/// everything. There is no "empty means everything" interpretation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ResourceScope {
    All,
    Restricted { resources: Vec<String> },
}

impl ResourceScope {
    /// Explicit unrestricted scope. Only for verified memberships.
    pub fn all() -> Self {
        ResourceScope::All
    }

    /// A scoped grant. An empty list denies all resources.
    pub fn restricted(resources: Vec<String>) -> Self {
        ResourceScope::Restricted { resources }
    }

    pub fn allows(&self, resource: &str) -> bool {
        match self {
            ResourceScope::All => true,
            ResourceScope::Restricted { resources } => {
                resources.iter().any(|r| r == resource)
            }
        }
    }
}

impl Default for ResourceScope {
    fn default() -> Self {
        // Deny by default: scoped credentials must list resources explicitly.
        ResourceScope::Restricted { resources: vec![] }
    }
}

/// The single authorization decision point for humans, agents and generated
/// apps. Binds the acting identity, the verified company context, the role
/// from the membership record and the resource scope.
#[derive(Debug, Clone)]
pub struct Authorization {
    pub actor: Actor,
    pub company_id: uuid::Uuid,
    pub role: Role,
    pub scope: ResourceScope,
}

impl Authorization {
    /// Verified company membership: full role permissions within the company.
    /// Callers must construct this from a server-side membership lookup, never
    /// from client-asserted identity.
    pub fn member(actor: Actor, company_id: uuid::Uuid, role: Role) -> Self {
        Authorization {
            actor,
            company_id,
            role,
            scope: ResourceScope::all(),
        }
    }

    /// Scoped action grant for an agent or generated app: role permissions
    /// narrowed to the listed resources. Empty resources deny everything.
    pub fn granted(
        actor: Actor,
        company_id: uuid::Uuid,
        role: Role,
        resources: Vec<String>,
    ) -> Self {
        Authorization {
            actor,
            company_id,
            role,
            scope: ResourceScope::restricted(resources),
        }
    }

    /// Check an action on a resource. Both the role permission set and the
    /// resource scope must allow it.
    pub fn check(&self, action: Permission, resource: &str) -> Result<()> {
        if !self.role.allows(action) {
            return Err(Error::Forbidden {
                action: action.as_str().to_string(),
                reason: format!("role {} does not grant {}", self.role, action.as_str()),
            });
        }
        if !self.scope.allows(resource) {
            return Err(Error::Forbidden {
                action: action.as_str().to_string(),
                reason: format!("resource {} is out of scope", resource),
            });
        }
        Ok(())
    }
}

/// Common platform error type. The API layer maps these to HTTP status codes;
/// logs never include secrets or raw provider errors.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found: {entity}")]
    NotFound { entity: String },
    #[error("forbidden: {action} ({reason})")]
    Forbidden { action: String, reason: String },
    #[error("conflict: {message}")]
    Conflict { message: String },
    #[error("invalid input: {message}")]
    Invalid { message: String },
    #[error("internal error")]
    Internal(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Clock abstraction so tests can control time without sleeping.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn role_ordering_matches_names() {
        assert!(Role::Viewer < Role::Member);
        assert!(Role::Member < Role::Manager);
        assert!(Role::Manager < Role::Admin);
        assert!(Role::Admin < Role::Owner);
    }

    #[test]
    fn viewer_can_read_but_not_write() {
        assert!(Role::Viewer.allows(Permission::RecordsRead));
        assert!(!Role::Viewer.allows(Permission::RecordsWrite));
        assert!(!Role::Viewer.allows(Permission::MembersManage));
    }

    #[test]
    fn member_can_write_but_not_manage_members() {
        assert!(Role::Member.allows(Permission::RecordsWrite));
        assert!(!Role::Member.allows(Permission::RecordsDelete));
        assert!(!Role::Member.allows(Permission::MembersManage));
    }

    #[test]
    fn manager_can_delete_but_not_install_apps() {
        assert!(Role::Manager.allows(Permission::RecordsDelete));
        assert!(Role::Manager.allows(Permission::AgentsRun));
        assert!(!Role::Manager.allows(Permission::AppsInstall));
        assert!(!Role::Manager.allows(Permission::AgentsApprove));
    }

    #[test]
    fn admin_can_manage_but_not_transfer_company() {
        assert!(Role::Admin.allows(Permission::MembersManage));
        assert!(Role::Admin.allows(Permission::AppsInstall));
        assert!(Role::Admin.allows(Permission::AuditRead));
        assert!(!Role::Admin.allows(Permission::CompanyTransfer));
    }

    #[test]
    fn owner_has_every_permission() {
        for p in Permission::ALL {
            assert!(Role::Owner.allows(p), "owner must allow {}", p);
        }
    }

    #[test]
    fn permission_roundtrips_as_dotted_string() {
        for p in Permission::ALL {
            assert_eq!(Permission::parse(p.as_str()).expect("parse"), p);
            let json = serde_json::to_string(&p).expect("serialize");
            assert_eq!(json, format!("\"{}\"", p.as_str()));
            assert_eq!(serde_json::from_str::<Permission>(&json).expect("deserialize"), p);
        }
    }

    #[test]
    fn unknown_permission_is_rejected_not_defaulted() {
        assert!(Permission::parse("records.wipe").is_err());
        assert!(serde_json::from_str::<Permission>("\"root.all\"").is_err());
    }

    #[test]
    fn membership_is_unrestricted_but_still_role_bounded() {
        let auth = Authorization::member(Actor::System, Uuid::new_v4(), Role::Member);
        assert!(auth.check(Permission::RecordsWrite, "anything").is_ok());
        assert!(auth.check(Permission::MembersManage, "anything").is_err());
    }

    #[test]
    fn empty_scoped_grant_denies_everything() {
        let auth = Authorization::granted(
            Actor::Agent { id: Uuid::new_v4() },
            Uuid::new_v4(),
            Role::Admin,
            vec![],
        );
        assert!(auth.check(Permission::RecordsRead, "app:inventory").is_err());
        assert!(auth.check(Permission::RecordsRead, "*").is_err());
    }

    #[test]
    fn scoped_grant_narrows_role_and_resources() {
        let auth = Authorization::granted(
            Actor::GeneratedApp { app_id: Uuid::new_v4(), install_id: Uuid::new_v4() },
            Uuid::new_v4(),
            Role::Manager,
            vec!["app:inventory".into()],
        );
        assert!(auth.check(Permission::RecordsWrite, "app:inventory").is_ok());
        assert!(auth.check(Permission::RecordsWrite, "app:other").is_err());
        // The role boundary still applies inside the scope.
        assert!(auth.check(Permission::MembersManage, "app:inventory").is_err());
    }

    #[test]
    fn scope_serializes_explicitly_and_rejects_unknown_modes() {
        let all = serde_json::to_string(&ResourceScope::all()).unwrap();
        assert_eq!(all, "{\"mode\":\"all\"}");
        let restricted =
            serde_json::to_string(&ResourceScope::restricted(vec!["app:inventory".into()])).unwrap();
        assert_eq!(restricted, "{\"mode\":\"restricted\",\"resources\":[\"app:inventory\"]}");
        assert!(serde_json::from_str::<ResourceScope>("{\"mode\":\"wat\"}").is_err());
        // Empty restricted list is deny-all, not allow-all.
        let empty: ResourceScope = serde_json::from_str("{\"mode\":\"restricted\",\"resources\":[]}").unwrap();
        assert!(!empty.allows("anything"));
    }
}
