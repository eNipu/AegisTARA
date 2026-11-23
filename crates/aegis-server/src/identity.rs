//! Qualified authorization identities and stable internal user IDs (220).
//!
//! Local password accounts and OIDC `(issuer, sub)` identities are never linked
//! implicitly by matching strings. Each external identity maps to a stable
//! internal [`UserId`] (`uid:<uuid>`) used for project memberships and new
//! audit attribution, while historical audit bytes remain untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

use crate::auth::{AuthMethod, Principal};
use crate::ids::ProjectId;
use crate::store::ProjectMembership;

/// Stable internal user identifier (`uid:<uuid>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserId(Uuid);

impl UserId {
    /// Generates a new time-ordered internal user ID (UUIDv7).
    #[must_use]
    pub fn new_v7() -> Self {
        Self(Uuid::now_v7())
    }

    /// Wraps an existing UUID.
    #[must_use]
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// Underlying UUID.
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0
    }

    /// The canonical membership and audit key: `uid:<uuid>`.
    #[must_use]
    pub fn member_key(self) -> String {
        format!("uid:{}", self.0)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A qualified external identity presented at authentication time.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExternalIdentity {
    /// A local password account (`AEGIS_AUTH=password` or `both`).
    Local {
        /// Normalized local username.
        username: String,
    },
    /// An OIDC caller identified by both issuer URL and `sub` claim.
    Oidc {
        /// Normalized issuer URL (`iss`).
        issuer: String,
        /// Provider-scoped subject (`sub`).
        subject: String,
    },
    /// A development-header caller (development/standalone only).
    Development {
        /// Development header user string.
        user: String,
    },
}

impl ExternalIdentity {
    /// Extracts the qualified external identity of a [`Principal`].
    #[must_use]
    pub fn from_principal(who: &Principal) -> Self {
        match who.method {
            AuthMethod::Password => Self::Local {
                username: who.subject.to_string(),
            },
            AuthMethod::Oidc => Self::Oidc {
                issuer: who.issuer.as_deref().unwrap_or("").to_owned(),
                subject: who.subject.to_string(),
            },
            AuthMethod::Development => Self::Development {
                user: who.subject.to_string(),
            },
        }
    }

    /// The bare legacy subject string that old `user:<bare>` memberships used.
    #[must_use]
    pub fn bare_name(&self) -> &str {
        match self {
            Self::Local { username } => username,
            Self::Oidc { subject, .. } => subject,
            Self::Development { user } => user,
        }
    }
}

/// Errors from explicit identity linking.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    /// The external identity is already bound to another internal user ID.
    #[error("external identity is already linked to another user ({existing})")]
    AlreadyLinked {
        /// Existing user ID that owns the binding.
        existing: UserId,
    },
}

/// Maps qualified [`ExternalIdentity`] values to stable internal [`UserId`]s.
///
/// Many external identities may be explicitly linked to one [`UserId`] (for
/// example during local-to-OIDC migration or IdP issuer replacement), but two
/// external identities are never linked implicitly by equal bare strings.
#[derive(Clone, Debug, Default)]
pub struct IdentityDirectory {
    bindings: Arc<RwLock<BTreeMap<ExternalIdentity, UserId>>>,
}

impl IdentityDirectory {
    /// Creates an empty directory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the [`UserId`] bound to `external`, provisioning a new one if
    /// none exists yet. Never links across different external identities.
    #[must_use]
    pub fn resolve_or_provision(&self, external: ExternalIdentity) -> UserId {
        let mut guard = self
            .bindings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard.entry(external).or_insert_with(UserId::new_v7)
    }

    /// Looks up the [`UserId`] for `external` without provisioning one.
    #[must_use]
    pub fn get(&self, external: &ExternalIdentity) -> Option<UserId> {
        let guard = self
            .bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.get(external).copied()
    }

    /// Attaches the resolved [`UserId`] to `principal`.
    #[must_use]
    pub fn qualify(&self, mut principal: Principal) -> Principal {
        let uid = self.resolve_or_provision(ExternalIdentity::from_principal(&principal));
        principal.user_id = Some(uid);
        principal
    }

    /// Explicitly links `external` to an existing internal `target` user ID.
    /// Idempotent when `external` is already bound to `target`.
    ///
    /// # Errors
    ///
    /// [`IdentityError::AlreadyLinked`] if `external` is already bound to a
    /// different [`UserId`].
    pub fn link(&self, external: ExternalIdentity, target: UserId) -> Result<(), IdentityError> {
        let mut guard = self
            .bindings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&existing) = guard.get(&external) {
            if existing == target {
                return Ok(());
            }
            return Err(IdentityError::AlreadyLinked { existing });
        }
        guard.insert(external, target);
        Ok(())
    }

    /// Re-binds every OIDC identity under `old_issuer` to `new_issuer` while
    /// keeping the exact same internal [`UserId`]s. Returns how many bindings
    /// were migrated.
    pub fn replace_issuer(&self, old_issuer: &str, new_issuer: &str) -> usize {
        if old_issuer == new_issuer {
            return 0;
        }
        let mut guard = self
            .bindings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let to_move: Vec<(ExternalIdentity, UserId)> = guard
            .iter()
            .filter_map(|(k, &v)| match k {
                ExternalIdentity::Oidc { issuer, subject } if issuer == old_issuer => Some((
                    ExternalIdentity::Oidc {
                        issuer: new_issuer.to_owned(),
                        subject: subject.clone(),
                    },
                    v,
                )),
                _ => None,
            })
            .collect();
        guard.retain(
            |k, _| !matches!(k, ExternalIdentity::Oidc { issuer, .. } if issuer == old_issuer),
        );
        let count = to_move.len();
        for (new_key, uid) in to_move {
            guard.insert(new_key, uid);
        }
        count
    }

    /// Returns every distinct [`UserId`] currently bound to an external identity
    /// whose bare name equals `bare`.
    #[must_use]
    pub fn candidates_for_bare(&self, bare: &str) -> Vec<UserId> {
        let guard = self
            .bindings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ids: BTreeSet<UserId> = guard
            .iter()
            .filter(|(k, _)| k.bare_name() == bare)
            .map(|(_, &uid)| uid)
            .collect();
        ids.into_iter().collect()
    }
}

/// A legacy `user:<bare>` membership row that could not be unambiguously mapped
/// to a single [`UserId`] during migration. It is quarantined and grants no
/// access until an administrator explicitly resolves it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmbiguousMembership {
    /// Project owning the membership row.
    pub project: ProjectId,
    /// Original bare name after `user:`.
    pub bare_subject: String,
    /// Role held by the legacy row.
    pub role: String,
    /// Candidate [`UserId`]s matching `bare_subject` (empty = unknown, >1 = collision).
    pub candidates: Vec<UserId>,
}

/// Result of migrating a membership table from legacy `user:<bare>` keys to
/// qualified `uid:<uuid>` keys. Includes the exact pre-migration `backup` for
/// rollback, the `rewritten` rows, and any `ambiguous` rows requiring explicit
/// administrator resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationPlan {
    /// Exact pre-migration rows for rollback.
    pub backup: Vec<ProjectMembership>,
    /// Post-migration rows: unambiguous legacy rows are rewritten to
    /// `uid:<uuid>`, `group:` and existing `uid:` rows are preserved, and
    /// ambiguous legacy rows remain quarantined (never granting access to
    /// local or OIDC callers).
    pub rows: Vec<ProjectMembership>,
    /// Ambiguous legacy rows that require explicit resolution.
    pub ambiguous: Vec<AmbiguousMembership>,
}

impl MigrationPlan {
    /// Returns the exact pre-migration membership table for rollback.
    #[must_use]
    pub fn rollback_rows(&self) -> &[ProjectMembership] {
        &self.backup
    }
}

/// Migrates legacy `user:<bare>` membership rows to `uid:<uuid>` using
/// `directory`.
///
/// - Unambiguous `user:<bare>` rows (matching exactly one [`UserId`]) are
///   rewritten to `uid:<uuid>`.
/// - Ambiguous `user:<bare>` rows (0 or >1 matching [`UserId`]s) are recorded
///   in [`MigrationPlan::ambiguous`] and left as un-matched legacy keys so they
///   never implicitly grant access to local or OIDC callers.
/// - Already-qualified `uid:<uuid>` rows and `group:<name>` rows pass through
///   unchanged, making migration retries idempotent.
#[must_use]
pub fn migrate_legacy_memberships(
    existing: &[ProjectMembership],
    directory: &IdentityDirectory,
) -> MigrationPlan {
    let backup = existing.to_vec();
    let mut rows = Vec::with_capacity(existing.len());
    let mut ambiguous = Vec::new();
    let mut seen: BTreeSet<(ProjectId, String)> = BTreeSet::new();

    for row in existing {
        if let Some(bare) = row.member.strip_prefix("user:") {
            let candidates = directory.candidates_for_bare(bare);
            if candidates.len() == 1 {
                let qualified = candidates[0].member_key();
                if seen.insert((row.project, qualified.clone())) {
                    rows.push(ProjectMembership {
                        project: row.project,
                        member: qualified,
                        role: row.role.clone(),
                    });
                }
            } else {
                ambiguous.push(AmbiguousMembership {
                    project: row.project,
                    bare_subject: bare.to_owned(),
                    role: row.role.clone(),
                    candidates,
                });
                if seen.insert((row.project, row.member.clone())) {
                    rows.push(row.clone());
                }
            }
        } else if seen.insert((row.project, row.member.clone())) {
            rows.push(row.clone());
        }
    }

    MigrationPlan {
        backup,
        rows,
        ambiguous,
    }
}

/// Explicitly resolves one quarantined legacy `user:<bare>` membership row on
/// `project` to `target` (`uid:<uuid>`). Returns `true` if a legacy row was
/// replaced.
pub fn resolve_legacy_membership(
    rows: &mut [ProjectMembership],
    project: ProjectId,
    bare_subject: &str,
    target: UserId,
) -> bool {
    let legacy = format!("user:{bare_subject}");
    let qualified = target.member_key();
    let mut resolved = false;
    for row in rows.iter_mut() {
        if row.project == project && row.member == legacy {
            row.member.clone_from(&qualified);
            resolved = true;
        }
    }
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::{Access, Permission};

    fn local_principal(username: &str) -> Principal {
        Principal {
            subject: username.into(),
            issuer: None,
            user_id: None,
            admin: false,
            method: AuthMethod::Password,
            groups: Vec::new(),
        }
    }

    fn oidc_principal(issuer: &str, subject: &str) -> Principal {
        Principal {
            subject: subject.into(),
            issuer: Some(issuer.into()),
            user_id: None,
            admin: false,
            method: AuthMethod::Oidc,
            groups: Vec::new(),
        }
    }

    #[test]
    fn equal_local_username_and_oidc_subject_never_share_identity_or_access() {
        let dir = IdentityDirectory::new();
        let local = dir.qualify(local_principal("alice"));
        let oidc = dir.qualify(oidc_principal("https://idp-a.example", "alice"));
        assert_ne!(local.user_id, oidc.user_id);
        assert_ne!(local.qualified_member_key(), oidc.qualified_member_key());
        assert_ne!(local.audit_actor(), oidc.audit_actor());

        let access = Access::new();
        let project = ProjectId::new_v7();
        access.set(&ProjectMembership {
            project,
            member: local.qualified_member_key(),
            role: "project_owner".into(),
        });
        assert!(access.allows(project, &local, Permission::Write));
        assert!(
            !access.allows(project, &oidc, Permission::Read),
            "OIDC caller with same bare subject must not inherit local access"
        );
    }

    #[test]
    fn two_issuers_with_same_subject_stay_isolated() {
        let dir = IdentityDirectory::new();
        let a = dir.qualify(oidc_principal("https://idp-a.example", "sub-42"));
        let b = dir.qualify(oidc_principal("https://idp-b.example", "sub-42"));
        assert_ne!(a.user_id, b.user_id);

        let access = Access::new();
        let project = ProjectId::new_v7();
        access.set(&ProjectMembership {
            project,
            member: a.qualified_member_key(),
            role: "analyst".into(),
        });
        assert!(access.allows(project, &a, Permission::Write));
        assert!(!access.allows(project, &b, Permission::Read));
    }

    #[test]
    fn issuer_replacement_preserves_internal_user_id_and_membership() {
        let dir = IdentityDirectory::new();
        let before = dir.qualify(oidc_principal("https://old-idp.example", "sub-7"));
        let uid = before.user_id.expect("uid");

        let access = Access::new();
        let project = ProjectId::new_v7();
        access.set(&ProjectMembership {
            project,
            member: uid.member_key(),
            role: "analyst".into(),
        });

        let moved = dir.replace_issuer("https://old-idp.example", "https://new-idp.example");
        assert_eq!(moved, 1);

        let after = dir.qualify(oidc_principal("https://new-idp.example", "sub-7"));
        assert_eq!(after.user_id, Some(uid));
        assert!(access.allows(project, &after, Permission::Write));

        // Old issuer no longer maps to `uid`.
        let stale = dir.qualify(oidc_principal("https://old-idp.example", "sub-7"));
        assert_ne!(stale.user_id, Some(uid));
        assert!(!access.allows(project, &stale, Permission::Read));
    }

    #[test]
    fn explicit_linking_binds_external_identities_and_refuses_conflicts() {
        let dir = IdentityDirectory::new();
        let local = dir.qualify(local_principal("dana"));
        let uid = local.user_id.expect("uid");

        let oidc_ext = ExternalIdentity::Oidc {
            issuer: "https://idp.example".into(),
            subject: "dana-oidc".into(),
        };
        dir.link(oidc_ext.clone(), uid).expect("explicit link");
        // Idempotent repeat.
        dir.link(oidc_ext.clone(), uid).expect("idempotent link");

        let other = UserId::new_v7();
        assert_eq!(
            dir.link(oidc_ext, other),
            Err(IdentityError::AlreadyLinked { existing: uid })
        );

        let oidc = dir.qualify(oidc_principal("https://idp.example", "dana-oidc"));
        assert_eq!(oidc.user_id, Some(uid));
    }

    #[test]
    fn legacy_migration_quarantines_ambiguous_rows_supports_retries_and_rollback() {
        let dir = IdentityDirectory::new();
        let local_alice = dir.qualify(local_principal("alice"));
        let oidc_alice = dir.qualify(oidc_principal("https://idp.example", "alice"));
        let local_bob = dir.qualify(local_principal("bob"));
        let bob_uid = local_bob.user_id.expect("bob uid");

        let project = ProjectId::new_v7();
        let legacy = vec![
            ProjectMembership {
                project,
                member: "user:alice".into(),
                role: "project_owner".into(),
            },
            ProjectMembership {
                project,
                member: "user:bob".into(),
                role: "analyst".into(),
            },
            ProjectMembership {
                project,
                member: "group:security".into(),
                role: "reviewer".into(),
            },
        ];

        let plan = migrate_legacy_memberships(&legacy, &dir);
        assert_eq!(plan.rollback_rows(), legacy.as_slice());
        // `bob` is unambiguous -> rewritten to uid; `alice` is ambiguous -> quarantined.
        assert_eq!(plan.ambiguous.len(), 1);
        assert_eq!(plan.ambiguous[0].bare_subject, "alice");
        assert_eq!(plan.ambiguous[0].candidates.len(), 2);

        // Migration retry is idempotent.
        let retry = migrate_legacy_memberships(&plan.rows, &dir);
        assert_eq!(retry.rows, plan.rows);
        assert_eq!(retry.ambiguous.len(), 1);

        // Ambiguous legacy `user:alice` grants access to NEITHER local nor OIDC alice.
        let access = Access::new();
        access.replace(plan.rows.clone());
        assert!(access.allows(project, &local_bob, Permission::Write));
        assert!(!access.allows(project, &local_alice, Permission::Read));
        assert!(!access.allows(project, &oidc_alice, Permission::Read));

        // Explicit admin resolution grants access only to the chosen user.
        let mut resolved_rows = plan.rows;
        assert!(resolve_legacy_membership(
            &mut resolved_rows,
            project,
            "alice",
            oidc_alice.user_id.expect("oidc uid"),
        ));
        access.replace(resolved_rows);
        assert!(access.allows(project, &oidc_alice, Permission::Manage));
        assert!(!access.allows(project, &local_alice, Permission::Read));
        assert_eq!(bob_uid.member_key(), local_bob.qualified_member_key());
    }
}
