# ADR 0048: Downloading the installer after an update check

- Status: accepted (job 054-installer-download)
- Date: 2026-10-26
- Scope: `crates/aegis-license/src/manifest.rs`, `crates/aegis-auth/src/http.rs`, `crates/aegis-client/src/{updates,session,commands}.rs`, `crates/aegis-shell` (Help menu)
- Rules: R3, R4, R6 in `docs/engineering-standards.md`; decision D5
- Builds on: `0042-update-manifest-fetch.md`, `0034-release-scaffolding.md`

## Context

ADR 0042 left the download out: an installer can be hundreds of megabytes, and
the HTTP client keeps whole bodies in memory with a 1 MiB limit. A user had to
fetch the file by hand and the app never checked it.

## Decisions

### 1. A streaming download, separate from `send`

`HttpDownload::download(url, max_bytes, sink)` passes the body to a closure
piece by piece. Memory is O(1). It refuses a body whose `content-length` is
over `max_bytes` before reading it, cuts a body that outgrows `max_bytes`
(chunked bodies have no length to check), gives up after 30 s of silence and
after one hour in all, follows no redirects and keeps the cleartext rule of
`send` (loopback only). `HttpClient::send` and its 1 MiB limit are unchanged,
so login code cannot be made to read large bodies by accident.

### 2. The signed size is the limit

`UpdateOffer::download` passes the size from the signed manifest as
`max_bytes`. A mirror that sends more cannot make the client store more.

### 3. Check while it arrives, name only when it is right

`Artifact::start_check` returns a `DownloadCheck` that counts and hashes the
pieces (SHA-256) and refuses the piece that crosses the signed size. The
bytes go to `<name>.part`. Only when size and hash both match is the file
synced and renamed to its final name. On any failure the `.part` file is
removed and an earlier good file with the same name is left alone. So a file
with the final name has always been checked.

### 4. The server does not choose the file name

The name is `AegisTARA-<release>-<platform>` plus an extension from a fixed
list (`.exe`, `.msi`, `.dmg`, `.pkg`, `.deb`, `.rpm`, `.appimage`, `.zip`,
`.tar.gz`, `.msix`; otherwise `.bin`). Release and platform are signed, and
every character outside letters, digits, `.`, `-` and `_` becomes `_`, so
nothing can leave the folder.

### 5. Nothing is run

The app saves and checks the file and tells the user where it is. Running an
installer from inside the app needs a per platform step (elevation, closing the
app, replacing files) and is a separate decision. This keeps the update path
free of any code execution by the client.

### 6. Where, and the menu

The folder is the user's `Downloads` (else a private folder in the temporary
folder, section 7); `AEGIS_UPDATE_DIR` overrides it. Help gets "Download update". It uses the
offer from the last "Check for updates" and says "Check for updates first"
if there is none. The result is a notice with the file path. A second
download while one runs is refused with a notice. There is no progress bar yet
(the progress callback exists).

### 7. A folder other users can write to (hardening, same job)

The security review of this branch found one conditional weakness: without a
`Downloads` folder the installer went to the system temporary folder, which on
Linux (`/tmp`) every local user can write to. `File::create` follows a
symlink, so a local user who guessed the file name could plant a link to a file
the victim owns and have it overwritten with installer bytes. A planted folder
could also let that user swap the checked file before it was started. Fixed in
two parts:

- **The part file is made with `create_new`** (the operating system's
  exclusive create: it fails if anything exists at the path, a symlink or a
  dangling link included, and never follows one) under a name with the process
  number, the clock in nanoseconds and a counter: `<name>.<unique>.part`. If the
  name is taken, the next one is tried. This holds in every folder.
- **The fallback is `DownloadDir::PrivateIn(temp)`**: each download makes a new
  folder inside it with `mkdir` mode 0700 in one step (it fails on any existing
  path, so nothing planted is reused), and the installer lands in it. A folder
  made for a download that fails is removed again. A chosen or Downloads folder
  (`DownloadDir::Folder`) is the user's own and is used as it is. The final
  rename replaces a symlink at the final name instead of following it.

Residual: in a `Folder` that another user can write to, that user could still
delete the checked file or replace it after the rename. Giving someone else
write access to your own Downloads folder is outside this job.

## Consequences

- Trust still rests on the signed manifest alone: transport security (TLS to a
  host the OS trusts) is needed to reach the file but is not what makes it
  acceptable. A wrong certificate stops the download, a wrong file fails the
  hash.
- The disk write happens on the session's task. Pieces are small, but a very
  slow disk slows that task. A later job can move it to a blocking thread.
- Resuming an interrupted download is not built. The `.part` file is deleted.

## Tests

License: pieces, too many bytes, too few, wrong bytes. HTTP client: a 3 MiB
body in order, announced size over the limit (no byte kept), chunked body that
outgrows the limit, status 404, a failing sink, a silent server (paused clock),
cleartext to a remote host. Client over real TLS with the test authority: a good
file, wrong bytes of the right size, too many and too few bytes and an empty
body, an earlier good file kept, a 404, an untrusted certificate, an unwritable
folder, and the Help menu commands through a session. Hardening: a link or a planted file at
the part name is never written through (it fails on the old `File::create`), a
link at the final name is replaced and not followed, each download gets its own
0700 folder in a shared base, planted names in the base are skipped, and a failed
download leaves no folder behind. Stress: four parallel
downloads of `AEGIS_STRESS_DOWNLOAD_MB` megabytes (1.2 GB in 6.8 s, release).
