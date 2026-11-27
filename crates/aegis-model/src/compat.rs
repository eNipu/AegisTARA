//! Engine and protocol compatibility contract (ADR 0075).
//!
//! `AegisTARA` separates three version axes that evolve independently:
//!
//! 1. **Engine calculation semantics** ([`ENGINE_SEMANTICS_VERSION`]): bumped
//!    whenever deterministic evaluation (attack tree evaluation, feasibility
//!    aggregation, risk matrix lookup, control mitigation, applicability, or
//!    lint rules) changes in a way that can alter computed outputs for the same
//!    pinned methodology pack and project state.
//! 2. **Wire/op protocol version** ([`PROTOCOL_VERSION`],
//!    [`MIN_SUPPORTED_PROTOCOL_VERSION`]): bumped whenever the gRPC API or
//!    operation schema changes incompatibly (for example a new required op
//!    variant or removed RPC field).
//! 3. **File format version** ([`FORMAT_VERSION`]): bumped only when the
//!    on-disk `aegis-project` / `aegis-oplog` framing changes.
//!
//! Methodology pack content is pinned separately by immutable digest
//! (`Methodology::pack_digest`, ADR 0074).

use std::fmt;

/// Monotonic identifier of deterministic calculation semantics.
///
/// Increment whenever a change to evaluation, mitigation, applicability, or
/// lint semantics could change any computed output for the same pinned
/// methodology pack and project state.
pub const ENGINE_SEMANTICS_VERSION: u32 = 2;

/// Current wire and operation protocol version spoken by this build.
pub const PROTOCOL_VERSION: u32 = 2;

/// Oldest wire and operation protocol version this server accepts for writes.
pub const MIN_SUPPORTED_PROTOCOL_VERSION: u32 = 2;

/// On-disk snapshot and operation-log file format version.
pub const FORMAT_VERSION: u32 = 1;

/// Compatibility contract advertised by a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineContract {
    /// Crate version string (`ENGINE_VERSION`).
    pub engine_version: String,
    /// Calculation semantics version (`0` means a legacy server that did not
    /// advertise compatibility metadata).
    pub engine_semantics: u32,
    /// Current protocol version spoken by the server (`0` for legacy servers).
    pub protocol_version: u32,
    /// Oldest protocol version accepted by the server (`0` for legacy servers).
    pub min_protocol_version: u32,
}

impl EngineContract {
    /// The compatibility contract of this compiled build.
    #[must_use]
    pub fn current() -> Self {
        Self {
            engine_version: crate::ENGINE_VERSION.to_owned(),
            engine_semantics: ENGINE_SEMANTICS_VERSION,
            protocol_version: PROTOCOL_VERSION,
            min_protocol_version: MIN_SUPPORTED_PROTOCOL_VERSION,
        }
    }
}

/// Compatibility identity advertised by a client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientContract {
    /// Crate version string (`ENGINE_VERSION`).
    pub engine_version: String,
    /// Calculation semantics version of this client.
    pub engine_semantics: u32,
    /// Protocol version spoken by this client.
    pub protocol_version: u32,
}

impl ClientContract {
    /// The compatibility identity of this compiled client build.
    #[must_use]
    pub fn current() -> Self {
        Self {
            engine_version: crate::ENGINE_VERSION.to_owned(),
            engine_semantics: ENGINE_SEMANTICS_VERSION,
            protocol_version: PROTOCOL_VERSION,
        }
    }
}

/// Why a client and server are not write-compatible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompatibilityError {
    /// Client and server implement different calculation semantics.
    EngineSemanticsMismatch {
        /// Semantics version in the client.
        client: u32,
        /// Semantics version in the server.
        server: u32,
    },
    /// Legacy server without explicit semantics metadata running a different
    /// engine version string.
    EngineVersionMismatch {
        /// Engine version in the client.
        client: String,
        /// Engine version in the server.
        server: String,
    },
    /// The client speaks a protocol version older than the server accepts.
    ProtocolTooOld {
        /// Protocol version in the client.
        client: u32,
        /// Minimum protocol version accepted by the server.
        server_min: u32,
        /// Current protocol version of the server.
        server_current: u32,
    },
    /// The client speaks a protocol version newer than the server understands.
    ProtocolTooNew {
        /// Protocol version in the client.
        client: u32,
        /// Current protocol version of the server.
        server_current: u32,
    },
    /// A request carried partial, malformed, or unsupported legacy headers.
    MalformedHeader(&'static str),
}

impl CompatibilityError {
    /// An actionable explanation suitable for end-user notices and error logs.
    #[must_use]
    pub fn actionable_reason(&self) -> String {
        match self {
            CompatibilityError::EngineSemanticsMismatch { client, server } => {
                if client < server {
                    format!(
                        "The server uses calculation engine semantics v{server}, while this app uses v{client}. Editing is off and server numbers are shown; update AegisTARA to edit."
                    )
                } else {
                    format!(
                        "This app uses calculation engine semantics v{client}, which is newer than the server's v{server}. Editing is off and server numbers are shown; ask your administrator to upgrade the server or use a matching client."
                    )
                }
            }
            CompatibilityError::EngineVersionMismatch { client, server } => format!(
                "The server runs engine {server} and this app runs {client}. Editing is off and server numbers are shown; update AegisTARA to match the server."
            ),
            CompatibilityError::ProtocolTooOld {
                client,
                server_min,
                server_current,
            } => format!(
                "This app speaks protocol v{client}, but the server requires protocol v{server_min}..=v{server_current}. Editing is off; update AegisTARA."
            ),
            CompatibilityError::ProtocolTooNew {
                client,
                server_current,
            } => format!(
                "This app speaks protocol v{client}, which is newer than the server's protocol v{server_current}. Editing is off; upgrade the server or use a compatible client."
            ),
            CompatibilityError::MalformedHeader(detail) => {
                format!("Client compatibility headers are invalid ({detail}). Update AegisTARA.")
            }
        }
    }
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.actionable_reason())
    }
}

impl std::error::Error for CompatibilityError {}

/// Checks whether a client may edit projects hosted by `server`.
///
/// Legacy servers that do not populate `engine_semantics` (`0`) fall back to
/// exact `engine_version` string comparison. Servers that advertise
/// `engine_semantics > 0` enforce exact calculation semantics equality and
/// protocol range `[min_protocol_version, protocol_version]`.
///
/// # Errors
///
/// Returns [`CompatibilityError`] with an actionable reason when the client
/// must stay read-only.
pub fn check_client_compatibility(
    client: &ClientContract,
    server: &EngineContract,
) -> Result<(), CompatibilityError> {
    if server.engine_semantics == 0 {
        if client.engine_version == server.engine_version {
            return Ok(());
        }
        return Err(CompatibilityError::EngineVersionMismatch {
            client: client.engine_version.clone(),
            server: server.engine_version.clone(),
        });
    }
    if client.engine_semantics != server.engine_semantics {
        return Err(CompatibilityError::EngineSemanticsMismatch {
            client: client.engine_semantics,
            server: server.engine_semantics,
        });
    }
    if server.min_protocol_version > 0 && client.protocol_version < server.min_protocol_version {
        return Err(CompatibilityError::ProtocolTooOld {
            client: client.protocol_version,
            server_min: server.min_protocol_version,
            server_current: server.protocol_version,
        });
    }
    if server.protocol_version > 0 && client.protocol_version > server.protocol_version {
        return Err(CompatibilityError::ProtocolTooNew {
            client: client.protocol_version,
            server_current: server.protocol_version,
        });
    }
    Ok(())
}

/// Checks whether an incoming write RPC carrying optional compatibility headers
/// is permitted against `server`.
///
/// Legacy clients that omit both headers are assumed to run semantics `1` and
/// protocol `1`. As long as the server runs semantics `1` and accepts protocol
/// `1`, legacy v1 requests remain accepted; once either bumps past `1`, legacy
/// requests without headers are rejected automatically. Sending only one of the
/// two headers is rejected as malformed.
///
/// # Errors
///
/// Returns [`CompatibilityError`] when the write must be refused.
pub fn check_server_write_compatibility(
    client_semantics: Option<u32>,
    client_protocol: Option<u32>,
    server: &EngineContract,
) -> Result<(), CompatibilityError> {
    let (semantics, protocol) = match (client_semantics, client_protocol) {
        (Some(s), Some(p)) => (s, p),
        (None, None) => (1, 1),
        (Some(_), None) => {
            return Err(CompatibilityError::MalformedHeader(
                "missing x-aegis-protocol-version",
            ));
        }
        (None, Some(_)) => {
            return Err(CompatibilityError::MalformedHeader(
                "missing x-aegis-engine-semantics",
            ));
        }
    };
    if semantics != server.engine_semantics {
        return Err(CompatibilityError::EngineSemanticsMismatch {
            client: semantics,
            server: server.engine_semantics,
        });
    }
    if protocol < server.min_protocol_version {
        return Err(CompatibilityError::ProtocolTooOld {
            client: protocol,
            server_min: server.min_protocol_version,
            server_current: server.protocol_version,
        });
    }
    if protocol > server.protocol_version {
        return Err(CompatibilityError::ProtocolTooNew {
            client: protocol,
            server_current: server.protocol_version,
        });
    }
    Ok(())
}

/// Formats a concise engine identity string for baselines and exported reports.
#[must_use]
pub fn format_engine_identity(engine_version: &str, engine_semantics: u32) -> String {
    if engine_semantics == 0 {
        engine_version.to_owned()
    } else {
        format!("{engine_version} (semantics v{engine_semantics})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_patch_release_with_same_semantics_and_protocol_is_allowed() {
        let server = EngineContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 2,
            min_protocol_version: 1,
        };
        let client = ClientContract {
            engine_version: "0.1.1".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
        };
        assert_eq!(check_client_compatibility(&client, &server), Ok(()));
    }

    #[test]
    fn mismatched_semantics_makes_client_read_only() {
        let server = EngineContract {
            engine_version: "0.2.0".to_owned(),
            engine_semantics: 2,
            protocol_version: 1,
            min_protocol_version: 1,
        };
        let client = ClientContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
        };
        let err = check_client_compatibility(&client, &server).unwrap_err();
        assert_eq!(
            err,
            CompatibilityError::EngineSemanticsMismatch {
                client: 1,
                server: 2
            }
        );
        assert!(err.actionable_reason().contains("update AegisTARA"));
    }

    #[test]
    fn protocol_too_old_or_too_new_is_rejected() {
        let server = EngineContract {
            engine_version: "0.3.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 3,
            min_protocol_version: 2,
        };
        let older = ClientContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
        };
        assert!(matches!(
            check_client_compatibility(&older, &server),
            Err(CompatibilityError::ProtocolTooOld {
                client: 1,
                server_min: 2,
                server_current: 3
            })
        ));
        let newer = ClientContract {
            engine_version: "0.4.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 4,
        };
        assert!(matches!(
            check_client_compatibility(&newer, &server),
            Err(CompatibilityError::ProtocolTooNew {
                client: 4,
                server_current: 3
            })
        ));
    }

    #[test]
    fn legacy_server_falls_back_to_engine_version_string() {
        let legacy_server = EngineContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 0,
            protocol_version: 0,
            min_protocol_version: 0,
        };
        let matching = ClientContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
        };
        assert_eq!(
            check_client_compatibility(&matching, &legacy_server),
            Ok(())
        );
        let different = ClientContract {
            engine_version: "0.2.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
        };
        assert!(matches!(
            check_client_compatibility(&different, &legacy_server),
            Err(CompatibilityError::EngineVersionMismatch { .. })
        ));
    }

    #[test]
    fn legacy_write_requests_succeed_on_v1_and_fail_after_bump() {
        let v1 = EngineContract {
            engine_version: "0.1.0".to_owned(),
            engine_semantics: 1,
            protocol_version: 1,
            min_protocol_version: 1,
        };
        assert_eq!(check_server_write_compatibility(None, None, &v1), Ok(()));

        let future_semantics = EngineContract {
            engine_semantics: ENGINE_SEMANTICS_VERSION + 1,
            ..v1.clone()
        };
        assert!(matches!(
            check_server_write_compatibility(None, None, &future_semantics),
            Err(CompatibilityError::EngineSemanticsMismatch { client: 1, server })
            if server == ENGINE_SEMANTICS_VERSION + 1
        ));

        let v2_protocol = EngineContract {
            protocol_version: 2,
            min_protocol_version: 2,
            ..v1
        };
        assert!(matches!(
            check_server_write_compatibility(None, None, &v2_protocol),
            Err(CompatibilityError::ProtocolTooOld {
                client: 1,
                server_min: 2,
                server_current: 2
            })
        ));

        // Headerless legacy writes map to the historical (1, 1). This build
        // runs semantics 2 (and requires protocol 2); the semantics check
        // runs first, so the refusal names the semantics mismatch.
        assert!(matches!(
            check_server_write_compatibility(None, None, &EngineContract::current()),
            Err(CompatibilityError::EngineSemanticsMismatch { client: 1, server })
            if server == ENGINE_SEMANTICS_VERSION
        ));
    }
}
