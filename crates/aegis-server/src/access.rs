//! Built-in roles and per-project membership (ACC-01 to ACC-03, decision
//! D19).
//!
//! The six roles are fixed here. A membership row is one member key — either
//! `user:<subject>` or `group:<SSO group name>` — mapped to one role on one
//! project. Every project RPC resolves the caller's effective roles from the
//! access table and checks the permission it needs; `AegisService` is the
//! only caller, so the check cannot be skipped accidentally.
//!
//! # Built-in role table
//!
//! | Role | Read | Write (edit) | Manage membership | Admin (users, lock, audit) |
//! |---|---|---|---|---|
//! | `admin` | yes | yes | yes | yes |
//! | `methodology_owner` | yes | no | no | no |
//! | `project_owner` | yes | yes | yes (non-owner rows) | no |
//! | `analyst` | yes | yes | no | no |
//! | `reviewer` | yes | no | no | no |
//! | `viewer` | yes | no | no | no |
//!
//! A server-global admin (the `Principal.admin` flag, from the OIDC admin
//! claim or the password admin flag) has every permission on every project
//! without a membership row. The admin flag is checked before the access
//! table, and the access table does not grant admin capabilities.
//!
//! # Membership roles map from SSO groups
//!
//! OIDC principals carry group names in [`Principal::groups`] (from the
//! `groups` and `roles` claims). A caller matches a membership row by
//! `user:<subject>` first, then by each `group:<name>` in claim order.
//!
//! # Keeping a usable table
//!
//! A project always has at least one owner row: a `project_owner` (or
//! `admin`) membership cannot be removed or demoted while it is the last
//! owner row, so nobody can lock a project out by accident.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::auth::Principal;
use crate::ids::ProjectId;
use crate::store::ProjectMembership;

/// Longest member key accepted on the wire, in bytes (`user:`/`group:` prefix
/// plus the subject or group name).
const MAX_MEMBER_BYTES: usize = 512;

/// Longest role key accepted on the wire, in bytes.
const MAX_ROLE_BYTES: usize = 64;

/// The six built-in roles (ACC-01). Ordered least to most capable.
pub const ROLE_NAMES: [&str; 6] = [
    "viewer",
    "reviewer",
    "analyst",
    "project_owner",
    "methodology_owner",
    "admin",
];

/// Whether a role key is one of the built-in roles.
#[must_use]
pub fn is_role(role: &str) -> bool {
    ROLE_NAMES.contains(&role)
}

/// A caller's permission to do something to a project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Permission {
    /// Open and read the project.
    Read,
    /// Change the project (take the lease, submit ops, rename, figures).
    Write,
    /// Change other members' roles on the project.
    Manage,
    /// Server-wide administration (users, sessions, break locks, delete
    /// projects, audit verification). Not granted by the access table.
    Admin,
}

/// The result of resolving a caller against a project's access table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessDecision {
    /// The role key that granted the decision, or `"admin"` for a global
    /// admin, or `None` when access was denied.
    pub role: Option<Arc<str>>,
}

/// The built-in role and membership table.
///
/// Reads against an in-memory cache rebuilt at startup and kept current by
/// the service (`D10`), so every RPC check is cheap and never touches the
/// store. The cache is keyed by project, then member key.
#[derive(Clone, Debug, Default)]
pub struct Access {
    rows: Arc<std::sync::RwLock<MembershipTable>>,
}

type MembershipTable = BTreeMap<ProjectId, BTreeMap<String, Arc<str>>>;

impl Access {
    /// An empty access table (used when `--no-access` is set or in dev).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the cached rows with `rows`.
    pub fn replace(&self, rows: Vec<ProjectMembership>) {
        let mut table: BTreeMap<ProjectId, BTreeMap<String, Arc<str>>> = BTreeMap::new();
        for row in rows {
            table
                .entry(row.project)
                .or_default()
                .insert(row.member, Arc::from(row.role));
        }
        *self.rows.write().unwrap_or_else(|p| p.into_inner()) = table;
    }

    /// Records or changes one membership row in the cache.
    pub fn set(&self, row: &ProjectMembership) {
        let mut guard = self.rows.write().unwrap_or_else(|p| p.into_inner());
        guard
            .entry(row.project)
            .or_default()
            .insert(row.member.clone(), Arc::from(row.role.as_str()));
    }

    /// Removes one membership row from the cache.
    pub fn remove(&self, project: ProjectId, member: &str) {
        let mut guard = self.rows.write().unwrap_or_else(|p| p.into_inner());
        if let Some(map) = guard.get_mut(&project) {
            map.remove(member);
            if map.is_empty() {
                guard.remove(&project);
            }
        }
    }

    /// The role keys a caller holds on a project, in `ROLE_NAMES` order.
    fn roles_for(&self, project: ProjectId, who: &Principal) -> Vec<Arc<str>> {
        let guard = self.rows.read().unwrap_or_else(|p| p.into_inner());
        let Some(map) = guard.get(&project) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        if let Some(role) = map.get(&format!("user:{}", who.subject)) {
            found.push(Arc::clone(role));
        }
        for group in &who.groups {
            if let Some(role) = map.get(&format!("group:{group}")) {
                found.push(Arc::clone(role));
            }
        }
        found.sort_by_key(|role| role_rank(role));
        found.dedup();
        found
    }

    /// Whether `who` may `permission` on `project`.
    #[must_use]
    pub fn allows(&self, project: ProjectId, who: &Principal, permission: Permission) -> bool {
        if who.admin {
            return true;
        }
        if permission == Permission::Admin {
            return false;
        }
        self.roles_for(project, who)
            .into_iter()
            .any(|role| role_allows(&role, permission))
    }

    /// The most capable role `who` holds on `project` (or `"admin"` when a
    /// global admin), if any.
    #[must_use]
    pub fn role_of(&self, project: ProjectId, who: &Principal) -> Option<Arc<str>> {
        if who.admin {
            return Some(Arc::from("admin"));
        }
        self.roles_for(project, who).into_iter().last()
    }
}

/// The rank of a role within `ROLE_NAMES`.
fn role_rank(role: &str) -> usize {
    ROLE_NAMES.iter().position(|r| *r == role).unwrap_or(0)
}

/// Whether a role grants a permission (roles never grant [`Permission::Admin`]).
fn role_allows(role: &str, permission: Permission) -> bool {
    matches!(
        (role, permission),
        (_, Permission::Read)
            | ("admin", Permission::Write | Permission::Manage)
            | ("project_owner", Permission::Write | Permission::Manage)
            | ("analyst", Permission::Write)
    )
}

/// A member key on the wire, either `user:<subject>` or `group:<name>`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member(String);

impl Member {
    /// Whether a wire member key is well formed.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Member> {
        if raw.len() > MAX_MEMBER_BYTES || raw.len() < 6 {
            return None;
        }
        let (kind, name) = raw.split_once(':')?;
        if !matches!(kind, "user" | "group") || name.is_empty() {
            return None;
        }
        if name.chars().any(char::is_control) {
            return None;
        }
        Some(Member(raw.to_owned()))
    }

    /// The stored key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The un-prefixed subject or group name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.0[self.0.find(':').unwrap_or(0) + 1..]
    }
}

/// Whether `role` is a valid built-in role key.
pub fn check_role(role: &str) -> bool {
    role.len() <= MAX_ROLE_BYTES && is_role(role)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn principal(subject: &str) -> Principal {
        Principal::user(subject)
    }

    #[test]
    fn role_table_is_total_and_ordered() {
        for role in ROLE_NAMES {
            assert!(is_role(role));
            assert!(role_allows(role, Permission::Read));
        }
        assert!(role_allows("project_owner", Permission::Write));
        assert!(role_allows("project_owner", Permission::Manage));
        assert!(!role_allows("analyst", Permission::Manage));
        assert!(!role_allows("reviewer", Permission::Write));
        assert!(!role_allows("viewer", Permission::Write));
    }

    #[test]
    fn roles_never_grant_admin() {
        for role in ROLE_NAMES {
            assert!(!role_allows(role, Permission::Admin));
        }
    }

    #[test]
    fn member_parse_rejects_bad_keys() {
        assert!(Member::parse("user:alice").is_some());
        assert!(Member::parse("group:eng").is_some());
        assert!(Member::parse("user:").is_none());
        assert!(Member::parse("alice").is_none());
        assert!(Member::parse("team:eng").is_none());
        assert!(Member::parse("user:a\u{0}b").is_none());
    }

    #[test]
    fn group_membership_and_global_admin() {
        let access = Access::new();
        let project = ProjectId::new_v7();
        access.set(&ProjectMembership {
            project,
            member: "group:eng".into(),
            role: "analyst".into(),
        });
        let mut who = principal("alice");
        who.groups = vec!["eng".into()];
        assert!(access.allows(project, &who, Permission::Write));
        assert!(!access.allows(project, &who, Permission::Manage));

        let mut root = principal("root");
        root.admin = true;
        assert!(access.allows(project, &root, Permission::Admin));
    }

    #[test]
    fn user_row_beats_group_row_and_most_capable_wins() {
        let access = Access::new();
        let project = ProjectId::new_v7();
        access.set(&ProjectMembership {
            project,
            member: "group:eng".into(),
            role: "viewer".into(),
        });
        access.set(&ProjectMembership {
            project,
            member: "user:alice".into(),
            role: "project_owner".into(),
        });
        let mut who = principal("alice");
        who.groups = vec!["eng".into()];
        assert_eq!(
            access.role_of(project, &who).as_deref(),
            Some("project_owner")
        );
        assert!(access.allows(project, &who, Permission::Manage));
    }
}
