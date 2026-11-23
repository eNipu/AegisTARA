# ADR 0075: Engine and Protocol Compatibility Contract

- **Status:** Accepted
- **Date:** 2026-11-23
- **Related:** ADR 0074 (Methodology Pins), `224-engine-compatibility`

## Context

Previously, `aegis-client` compared only `GetServerInfoResponse.engine_version`
(`CARGO_PKG_VERSION`) against its own `aegis_model::ENGINE_VERSION`, and the
server did not enforce compatibility on incoming write RPCs. This coupled four
separate concerns:

1. Packaging/release version (`CARGO_PKG_VERSION`)
2. Deterministic calculation semantics (attack-tree evaluation, feasibility
   aggregation, risk matrix lookup, control mitigation, applicability, lint)
3. Wire/operation protocol compatibility (gRPC RPCs and `Op` schema)
4. On-disk file format (`aegis-project` / `aegis-oplog` framing) and
   methodology pack content (pinned by digest in ADR 0074)

In addition, baselines and exported reports did not consistently record both the
engine semantics identity and the immutable methodology pack digest.

## Decision

### 1. Four Independent Version Axes

| Axis | Constant / Field | When it MUST change |
|---|---|---|
| **Engine calculation semantics** | `aegis_model::ENGINE_SEMANTICS_VERSION` (`u32`) | Any change to attack-tree evaluation, feasibility scoring, risk lookup, control mitigation, applicability, or lint rules that could produce a different result for the same pinned methodology pack and project state. |
| **Wire/op protocol** | `aegis_model::PROTOCOL_VERSION`, `MIN_SUPPORTED_PROTOCOL_VERSION` (`u32`) | Any incompatible change to gRPC RPCs or `Op` encoding/decoding (for example, a new required op variant or removed field). |
| **File format** | `aegis_model::FORMAT_VERSION` (`u32`) | Only when `aegis-project` or `aegis-oplog` framing changes incompatibly. |
| **Methodology pack content** | `Methodology::pack_digest()` (`sha256:...`) | Automatically changes whenever any semantic content of a methodology pack changes (ADR 0074). |

Patch releases that fix UI bugs or infrastructure issues without altering
calculation semantics or wire protocol keep the same `ENGINE_SEMANTICS_VERSION`
and `PROTOCOL_VERSION` and remain write-compatible.

### 2. Server Advertisement and Write Enforcement

- `GetServerInfoResponse` advertises `engine_semantics_version`,
  `protocol_version`, and `min_protocol_version` alongside `engine_version`.
- Normal clients (`aegis-client::Connection`) attach `x-aegis-engine-semantics`
  and `x-aegis-protocol-version` metadata headers to every write RPC.
- `AegisService` enforces `check_server_write_compatibility` on all mutating
  project RPCs (`CreateProject`, `DeleteProject`, `AcquireLock`, `RenewLock`,
  `SubmitOps`, `CreateBaseline`, `UploadFigure`, `DeleteFigure`).
- Incompatible writes are rejected with `FAILED_PRECONDITION` and
  `x-aegis-error: incompatible_client`.
- **Legacy request policy:** Pre-224 clients that omit both headers are treated
  as `(engine_semantics = 1, protocol_version = 1)`. While the server runs
  semantics `1` and accepts protocol `1`, those legacy clients continue to
  operate during the v1 compatibility window. As soon as the server bumps
  `ENGINE_SEMANTICS_VERSION > 1` or `MIN_SUPPORTED_PROTOCOL_VERSION > 1`,
  legacy requests without headers are rejected automatically. Requests that send
  only one of the two headers or non-numeric values are always rejected.

### 3. Client Read-Only Fallback

- On connect, `aegis-client::Session` evaluates `check_client_compatibility`
  against `ServerInfo::engine_contract()`.
- If incompatible (semantics mismatch, protocol too old, protocol too new, or
  version mismatch on a legacy server), the client stays **read-only**, displays
  the server's risk numbers instead of local numbers, disables editing, and
  surfaces the actionable reason from `CompatibilityError::actionable_reason()`.
- If a write RPC still returns `ServerCode::IncompatibleClient`, the session
  rejects the edit and surfaces the server's message rather than retrying.

### 4. Baseline and Report Evidence

- `BaselineMeta` and `pb::BaselineInfo` record `engine_version`,
  `engine_semantics`, `methodology_key`, `methodology_version`, and
  `methodology_digest` at creation time (defaulting to empty/`0` for pre-224
  baselines).
- `ReportDoc` (shared by HTML, PDF, and XLSX exports) records the engine
  identity (`<engine_version> (semantics v<engine_semantics>)`) and the pinned
  methodology pack digest (or `unpinned (legacy)` for legacy unpinned projects)
  on the cover table.
- AegisTARA records these identities as audit evidence and does **not** promise
  in-process execution of unavailable historical engines.

## Consequences

- Calculation changes cannot silently diverge between client and server even if
  `CARGO_PKG_VERSION` is unchanged or bumped independently.
- Incompatible clients cannot corrupt or write against an upgraded server even
  if they bypass `GetServerInfo`.
- Every new baseline and exported report carries verifiable engine and
  methodology pack provenance.
