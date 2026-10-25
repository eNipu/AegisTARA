# ADR 0039: Password login switch

- Status: accepted (job 045-password-login-switch)
- Date: 2026-10-25
- Scope: `crates/aegis-server/src/{accounts,auth_layer,service,main}.rs`, `crates/aegis-server/migrations/003_settings.sql`, `aegis.proto`, `crates/aegis-client/src/{connection,session,state,view}.rs`, `crates/aegis-shell/{ui/app.slint,src/launcher.rs}`
- Rules: C2, D10, Q3, R3 in `docs/engineering-standards.md`; decision D10
- Builds on: `0035-password-login.md`

## Context

Password login is for test servers and trials (D10). A trial that becomes a
customer deployment must stop accepting passwords, and the owner wants that
to be a setting an admin changes, not an environment variable and a restart.
The app then has to show only the company login.

## Decisions

### 1. A third mode, `AEGIS_AUTH=both`

`password` and `oidc` stay as they are. `both` runs company login (OIDC) and
password login side by side. `HybridAuthn` looks at the bearer token: the
`ats_` prefix means a password session, anything else goes to OIDC. A JWT
cannot start with `ats_`, so one kind of credential is never tried as the
other. The other option, a runtime switch inside `password` mode only, was
rejected: with no company login behind it, turning the form off would lock
everyone out.

### 2. The switch is a stored setting

A new table `aegis_setting (org_id, key, value)` holds `password_login`
(`on` or `off`; no row means on). `Accounts` keeps it in an `AtomicBool`
that every login and every request reads (no lock, no I/O). The value is
stored before the flag changes, so a crash cannot leave a switch that
disagrees with the database.

### 3. Turning it off ends the sessions

`set_password_login(false)` deletes every password session in memory and in
the database, so no open session outlives the switch. While it is off,
`authenticate` answers `None` and `log_in` answers `PasswordLoginOff` (code
`password_login_off`, `FAILED_PRECONDITION`) after the same early check, so a
refused login costs no Argon2 work. Turning it on again does not revive old
sessions. Streams that were open at that moment end when their next call or
lease renewal is refused (the same as for a disabled user).

### 4. No lock-out

Turning it off is refused with `no_company_login` unless the server was
started with a company login (`AccountsConfig.company_login`, set by
`both`). Only an admin may change it, from either login. If the company
login breaks, `AEGIS_PASSWORD_LOGIN=on` at start forces the switch on.
`AEGIS_PASSWORD_LOGIN=off` forces it off. The change is written to the event
log as `password_login_changed` with the actor and `on` or `off`.

### 5. The app asks the server, every time

`GetLoginOptions` gets a field `password_enabled`. For `password` it is
true, for `development` false, for `oidc` it follows the switch. The app
shows the company login. When `password_enabled` is set, one extra button
opens the password form, and the form has a button back. When the switch is
off, there is no way to the form. If someone is on the form while an admin
turns it off, a refused login sends them back to the company login with a
sentence that says why. An admin who turns it off while logged in with a
password is taken to the login screen at once.

## Consequences

- One more migration (003). Older servers do not know the fields; the app
  treats a missing `password_enabled` as false, which is the safe side.
- With `both`, an OIDC admin can use the user calls (`ListUsers` and others)
  because the accounts exist. The app shows the user table only for password
  sessions; this is a gap to look at in the UX review.
- No per-user or per-group rule: the switch is for the whole deployment.
