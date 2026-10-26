//! The event log as an administrator reads it (job 049, ADR 0043): plain
//! words for each kind of event, and a group to filter by.

use aegis_proto as pb;

use crate::time::format_ms;

/// What an administrator filters the event log by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventGroup {
    /// Logins, failed logins, logouts, password changes.
    Logins,
    /// Reports and tables written out.
    Exports,
    /// Users, projects, baselines and settings changed by someone.
    AdminActions,
    /// A kind this app does not know (a newer server).
    Other,
}

/// One line of the event log, in words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventRow {
    /// When, in UTC.
    pub when: String,
    /// Who did it.
    pub who: String,
    /// What happened.
    pub what: String,
    /// What it was done to: a user name or a project ID, or empty.
    pub about: String,
    /// For the filter.
    pub group: EventGroup,
}

/// One open session, for the administrator's session list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    /// What to pass back to end it.
    pub id: String,
    /// Whose session.
    pub user: String,
    /// The device name the app gave at login.
    pub device: String,
    /// When it began, in UTC, without the "UTC".
    pub started: String,
    /// When it ends by itself, in UTC, without the "UTC".
    pub expires: String,
}

/// One session in words. O(1).
#[must_use]
pub fn session_row(s: &pb::SessionInfo) -> SessionRow {
    SessionRow {
        id: s.id.clone(),
        user: s.username.clone(),
        device: if s.client_name.is_empty() {
            "unknown device".to_owned()
        } else {
            s.client_name.clone()
        },
        // The table titles say UTC, which saves room in narrow columns.
        started: format_ms(s.created_ms).trim_end_matches(" UTC").to_owned(),
        expires: format_ms(s.expires_ms).trim_end_matches(" UTC").to_owned(),
    }
}

/// Puts one event in words. O(1).
#[must_use]
pub fn event_row(e: &pb::EventInfo) -> EventRow {
    let d = e.detail.as_str();
    let (group, what) = match e.kind.as_str() {
        "login_ok" => (EventGroup::Logins, "Logged in".to_owned()),
        "login_failed" => (
            EventGroup::Logins,
            match d {
                "throttled" => "Login refused: too many failed tries".to_owned(),
                "password_login_off" => "Password login refused: it is turned off".to_owned(),
                _ => "Login failed".to_owned(),
            },
        ),
        "logout" => (EventGroup::Logins, "Logged out".to_owned()),
        "password_changed" => (EventGroup::Logins, "Changed their own password".to_owned()),
        "export" => (
            EventGroup::Exports,
            if d.is_empty() {
                "Exported a report or table".to_owned()
            } else {
                format!("Exported a report or table ({d})")
            },
        ),
        "user_created" => (EventGroup::AdminActions, "Created a user".to_owned()),
        "user_updated" => (
            EventGroup::AdminActions,
            if d.is_empty() {
                "Changed a user".to_owned()
            } else {
                format!("Changed a user ({d})")
            },
        ),
        "user_deleted" => (EventGroup::AdminActions, "Deleted a user".to_owned()),
        "project_deleted" => (EventGroup::AdminActions, "Deleted a project".to_owned()),
        "baseline_created" => (EventGroup::AdminActions, "Saved a baseline".to_owned()),
        "password_login_changed" => (
            EventGroup::AdminActions,
            format!("Turned password login {d}"),
        ),
        "session_revoked" => (
            EventGroup::AdminActions,
            if d.is_empty() {
                "Ended a session".to_owned()
            } else {
                format!("Ended a session ({d})")
            },
        ),
        other => (EventGroup::Other, other.to_owned()),
    };
    EventRow {
        when: format_ms(e.at_ms),
        who: e.actor.clone(),
        what,
        about: e.target.clone(),
        group,
    }
}

/// The rows of one group, in order, or all of them for `None`. O(n).
#[must_use]
pub fn filter(rows: &[EventRow], group: Option<EventGroup>) -> Vec<EventRow> {
    rows.iter()
        .filter(|r| group.is_none_or(|g| r.group == g))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: &str, actor: &str, target: &str, detail: &str) -> pb::EventInfo {
        pb::EventInfo {
            at_ms: 1_800_000_000_000,
            actor: actor.into(),
            kind: kind.into(),
            target: target.into(),
            detail: detail.into(),
        }
    }

    #[test]
    fn a_session_is_shown_with_a_device_or_a_placeholder() {
        let row = session_row(&pb::SessionInfo {
            id: "00ff00ff00ff00ff".into(),
            username: "bob".into(),
            client_name: String::new(),
            created_ms: 1_800_000_000_000,
            expires_ms: 1_800_043_200_000,
        });
        assert_eq!(row.device, "unknown device");
        assert_eq!(row.id, "00ff00ff00ff00ff");
        assert!(row.started.contains("2027"), "{}", row.started);
        assert_ne!(row.started, row.expires);
        assert!(!row.started.contains("UTC"), "{}", row.started);
    }

    #[test]
    fn every_kind_the_server_writes_has_words_and_a_group() {
        let cases = [
            ("login_ok", EventGroup::Logins),
            ("login_failed", EventGroup::Logins),
            ("logout", EventGroup::Logins),
            ("password_changed", EventGroup::Logins),
            ("export", EventGroup::Exports),
            ("user_created", EventGroup::AdminActions),
            ("user_updated", EventGroup::AdminActions),
            ("user_deleted", EventGroup::AdminActions),
            ("project_deleted", EventGroup::AdminActions),
            ("baseline_created", EventGroup::AdminActions),
            ("password_login_changed", EventGroup::AdminActions),
            ("session_revoked", EventGroup::AdminActions),
        ];
        for (kind, group) in cases {
            let row = event_row(&event(kind, "alice", "x", ""));
            assert_eq!(row.group, group, "{kind}");
            assert!(
                !row.what.is_empty() && row.what != kind,
                "{kind}: {}",
                row.what
            );
        }
    }

    #[test]
    fn a_kind_from_a_newer_server_is_shown_as_it_is() {
        let row = event_row(&event("quantum_audit", "alice", "", ""));
        assert_eq!(row.group, EventGroup::Other);
        assert_eq!(row.what, "quantum_audit");
    }

    #[test]
    fn details_are_put_in_words() {
        let off = event_row(&event("password_login_changed", "carol", "", "off"));
        assert!(off.what.contains("off"), "{}", off.what);
        let on = event_row(&event("password_login_changed", "carol", "", "on"));
        assert!(on.what.contains(" on"), "{}", on.what);
        let throttled = event_row(&event("login_failed", "bob", "bob", "throttled"));
        assert!(throttled.what.contains("too many"), "{}", throttled.what);
        let refused = event_row(&event("login_failed", "bob", "bob", "password_login_off"));
        assert!(refused.what.contains("turned off"), "{}", refused.what);
        let plain = event_row(&event("login_failed", "bob", "bob", ""));
        assert!(plain.what.contains("failed"), "{}", plain.what);
        let export = event_row(&event("export", "alice", "p1", "csv"));
        assert!(export.what.contains("csv"), "{}", export.what);
        let update = event_row(&event("user_updated", "root", "bob", "admin,disabled"));
        assert!(update.what.contains("admin"), "{}", update.what);
    }

    #[test]
    fn the_row_carries_time_actor_and_target() {
        let row = event_row(&event("user_created", "root", "bob", ""));
        assert_eq!(row.when, "15 Jan 2027 08:00 UTC");
        assert_eq!(row.who, "root");
        assert_eq!(row.about, "bob");
    }

    #[test]
    fn filtering_keeps_the_order_and_all_shows_everything() {
        let rows: Vec<_> = [
            event("login_ok", "a", "a", ""),
            event("export", "a", "p", "csv"),
            event("user_created", "r", "b", ""),
            event("export", "b", "p", "html"),
        ]
        .iter()
        .map(event_row)
        .collect();
        assert_eq!(filter(&rows, None).len(), 4);
        let exports = filter(&rows, Some(EventGroup::Exports));
        assert_eq!(exports.len(), 2);
        assert!(exports.iter().all(|r| r.group == EventGroup::Exports));
        assert_eq!(exports[0].who, "a");
        assert_eq!(exports[1].who, "b");
    }
}
