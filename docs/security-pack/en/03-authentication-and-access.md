# 3. Authentication and access

## 3.1 Summary

Users log in in one of two ways. Company login uses the customer's identity
provider (OpenID Connect) and is the way to run the product in production.
Password login is built in, for tests, trials and small sites. Both can run
side by side, and an administrator can turn password login off. After login,
every call is checked against the user's role in the project. There are six
built-in roles. One person edits a project at a time. Standalone mode has no
login and listens only on the loopback address of the user's own computer.
The product has no multi-factor login of its own; that comes from the
company identity provider.

## 3.2 Login methods

| Method | How it works | Notes |
|---|---|---|
| Company login | OpenID Connect code flow with PKCE, in the system browser. Tokens are bound to the device with DPoP: every call carries a proof signed by a device key. | MFA and account rules come from the customer's identity provider. `AEGIS_DPOP=optional` accepts tokens that are not bound to a device. This is weaker, because whoever holds the token can use it. The default is `required`. |
| Password login | A user name and password kept by the server. The server hands out a session token. | For tests, trials and small sites. No e-mail reset and no MFA. |
| Both side by side | `AEGIS_AUTH=both`. The server tells the two apart by the token. | An administrator turns password login off in the app. It cannot be turned off if there is no company login, so nobody is locked out. `AEGIS_PASSWORD_LOGIN` forces the value at start. |
| Development header | `AEGIS_DEV_AUTH=1` trusts a header that names the user. | Development only. It skips project permission checks. |

What the release checklist requires for a customer:

- `AEGIS_DEV_AUTH` and `AEGIS_DEV_LICENSE` are unset in the customer's
  environment.
- The login method is agreed. Company login (`AEGIS_AUTH=oidc`) for
  production, or password login for a trial only. A trial that moves to
  company login runs `AEGIS_AUTH=both`, and an administrator turns password
  login off.

## 3.3 Password login

- A new password has at least 10 characters and at most 256 bytes. It must
  not be only spaces and must not be the user name. There are no composition
  rules.
- Passwords are hashed with Argon2id (19 MiB, 2 passes, 1 lane). An old hash
  is hashed again with the current settings at the next login.
- A login with an unknown user name is checked against a decoy hash, so it
  takes the same time as a known one.
- Throttling per user name: 5 failures are free, then the wait doubles from
  1 second up to 15 minutes.
- Throttling per network address: at most 2 login checks at once from one
  address, and after 20 failures from one address that address is slowed
  down.
- Password hashing runs in a limited pool, so a flood of logins cannot use
  up the server. A login that waits more than 5 seconds for the pool is
  refused.
- There is no e-mail reset and no MFA. MFA comes from the company identity
  provider, with company login.
- The log records successful and failed logins, logouts and password
  changes. It never holds a password or token.

## 3.4 Sessions

- A session token is `ats_` plus 32 random bytes. The server stores only its
  SHA-256.
- A session lasts 12 hours by default (`AEGIS_SESSION_HOURS`, 1 to 720). The
  end time is absolute. Use does not extend it.
- A user has at most 10 sessions. When an eleventh opens, the oldest closes.
- A session ends at logout, at expiry, when an administrator ends it, when
  the user is disabled, when the password changes, and when password login
  is turned off.
- "Keep me logged in" stores the session token in the operating system
  keychain. The product never stores the password.

## 3.5 Roles and permissions

There are four permissions: read, write, manage (members and project
settings) and server administration. There are six built-in roles, from least
to most capable.

| Role | Read | Write | Manage | Server administration |
|---|---|---|---|---|
| `viewer` | yes | no | no | no |
| `reviewer` | yes | no | no | no |
| `analyst` | yes | yes | no | no |
| `project_owner` | yes | yes | yes | no |
| `methodology_owner` | yes | no | no | no |
| `admin` | yes | yes | yes | no |

- No project role grants server administration. A server administrator (from
  the company login claim, or the password administrator flag) has every
  permission in every project, without a membership.
- Membership is per project, for a user or for a group from the identity
  provider. A user holds the most capable role they get from their own entry
  and their groups.
- The person who creates a project becomes its owner. A project always keeps
  one owner: the last owner cannot be removed or demoted.
- The last enabled administrator cannot be removed, disabled or demoted.
- A project that has no membership rows is not visible to a user who is not
  an administrator, until an administrator adds a membership.

## 3.6 Editing lock

One person edits a project at a time. The lock expires after 120 seconds by
default. It is renewed while the editor works, so a lock is lost soon after
a crash. Other users read the last saved version. An administrator can break
a lock.

## 3.7 Standalone mode

- There is no login.
- The embedded server listens on `127.0.0.1` only, on a random port.
- It starts only after the license file checks out.
- The trade-off: any process of the same operating system user on that
  machine that finds the port can act as the user. The port is not reachable
  from the network.

## 3.8 Limits to know

- Password login has no multi-factor login and no e-mail reset.
- Users are created by an administrator or come with company login. There is
  no SCIM and no automatic user provisioning. Nothing of this is built.
- Roles are built in and cannot be edited. The `methodology_owner` role
  currently grants read access only.
- The lock lifetime is a server setting, not a per-project one.
- `Customer` runs the identity provider, its MFA rules and its group
  claims.

## 3.9 Sources

- `docs/architecture.md`
- `docs/release-checklist.md`
- `docs/adr/0028-login.md`
- `docs/adr/0032-license-tokens.md`
- `docs/adr/0054-security-hardening-sweep.md`
- `docs/adr/0061-project-access-roles-and-membership.md`
- `crates/aegis-server/src/main.rs`
- `crates/aegis-server/src/config.rs`
- `crates/aegis-server/src/access.rs`
- `crates/aegis-server/src/accounts/mod.rs`
- `crates/aegis-server/src/accounts/password.rs`
- `crates/aegis-shell/src/standalone.rs`
