//! Vendor tool: make keys, sign licenses, read them back.
//!
//! ```text
//! aegis-license keygen --key-id ID --out KEYFILE
//! aegis-license sign --key KEYFILE --key-id ID --id LICENSE_ID --customer NAME
//!                    --seats N --days N [--grace-days N] [--feature NAME]...
//!                    [--start UNIX_SECONDS]
//! aegis-license inspect FILE --keys KEYS_FILE
//! aegis-license sign-update --key KEYFILE --key-id ID --in MANIFEST.json
//! aegis-license inspect-update FILE --keys KEYS_FILE
//! aegis-license sign-bundle DIR --key KEYFILE
//! aegis-license verify-bundle DIR --keys KEYS_FILE
//! aegis-license public-pem --keys KEYS_FILE --key-id ID
//! ```
//!
//! `sign-bundle` writes `SHA256SUMS` and `SHA256SUMS.sig` in an offline
//! bundle folder (use the update key). `verify-bundle` checks the folder.
//! `public-pem` prints a key as PEM for `openssl pkeyutl -verify -rawin`.
//!
//! `sign-update` reads the manifest as JSON (the `Manifest` fields; its
//! `kid` is set from `--key-id`) and prints the signed file. Use a different
//! key for updates than for licenses.
//!
//! `sign` prints the license file. `keygen` writes the private key (mode
//! 0600 where the system has modes) and prints the line for `vendor.keys`.
//! The private key never goes in the repository.

use std::process::ExitCode;

use aegis_license::bundle;
use aegis_license::{DAY, Issuer, License, LicenseState, Manifest, TrustedKeys};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

fn flags<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].as_str())
        .collect()
}

fn need<'a>(args: &'a [String], name: &str) -> Result<&'a str, String> {
    flag(args, name).ok_or_else(|| format!("{name} is required"))
}

fn number(args: &[String], name: &str) -> Result<u64, String> {
    need(args, name)?
        .parse()
        .map_err(|_| format!("{name} must be a whole number"))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("keygen") => keygen(args),
        Some("sign") => sign(args),
        Some("inspect") => inspect(args),
        Some("sign-update") => sign_update(args),
        Some("inspect-update") => inspect_update(args),
        Some("sign-bundle") => sign_bundle(args),
        Some("verify-bundle") => verify_bundle(args),
        Some("public-pem") => public_pem(args),
        _ => Err(
            "use keygen, sign, inspect, sign-update, inspect-update, sign-bundle, \
             verify-bundle, or public-pem (see the top of this file)"
                .to_owned(),
        ),
    }
}

fn keygen(args: &[String]) -> Result<(), String> {
    let id = need(args, "--key-id")?;
    let out = need(args, "--out")?;
    let (pkcs8, public) = Issuer::generate().map_err(|e| e.to_string())?;
    write_private(out, &pkcs8)?;
    // The line is checked the same way the build checks it.
    let line = format!("{id} {}", encode(public));
    TrustedKeys::parse(&line).map_err(|e| e.to_string())?;
    println!("{line}");
    eprintln!("private key written to {out}; keep it offline and never commit it");
    Ok(())
}

fn encode(bytes: [u8; 32]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn write_private(path: &str, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    // create_new: never overwrite an existing key.
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("cannot create {path}: {e}"))?;
    file.write_all(bytes)
        .map_err(|e| format!("cannot write {path}: {e}"))
}

fn sign(args: &[String]) -> Result<(), String> {
    let key =
        std::fs::read(need(args, "--key")?).map_err(|e| format!("cannot read the key: {e}"))?;
    let issuer = Issuer::from_pkcs8(&key).map_err(|e| e.to_string())?;
    let start = match flag(args, "--start") {
        Some(_) => number(args, "--start")?,
        None => now(),
    };
    let days = number(args, "--days")?;
    let mut features: Vec<String> = flags(args, "--feature")
        .into_iter()
        .map(str::to_owned)
        .collect();
    features.sort();
    features.dedup();
    let license = License {
        version: 1,
        id: need(args, "--id")?.to_owned(),
        kid: need(args, "--key-id")?.to_owned(),
        customer: need(args, "--customer")?.to_owned(),
        seats: u32::try_from(number(args, "--seats")?)
            .map_err(|_| "--seats is too large".to_owned())?,
        issued_at: start,
        expires_at: start.saturating_add(days.saturating_mul(DAY)),
        grace_days: match flag(args, "--grace-days") {
            Some(_) => u32::try_from(number(args, "--grace-days")?)
                .map_err(|_| "--grace-days is too large".to_owned())?,
            None => 30,
        },
        features,
    };
    println!("{}", issuer.sign(&license).map_err(|e| e.to_string())?);
    Ok(())
}

fn inspect(args: &[String]) -> Result<(), String> {
    let file = args.get(1).ok_or("inspect needs a license file")?;
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let keys_text = std::fs::read_to_string(need(args, "--keys")?)
        .map_err(|e| format!("cannot read the key list: {e}"))?;
    let keys = TrustedKeys::parse(&keys_text).map_err(|e| e.to_string())?;
    let license = License::verify(&text, &keys).map_err(|e| e.to_string())?;
    println!("id:        {}", license.id);
    println!("customer:  {}", license.customer);
    println!("seats:     {}", license.seats);
    println!("issued at: {}", license.issued_at);
    println!("expires:   {}", license.expires_at);
    println!("grace:     {} days", license.grace_days);
    println!("features:  {}", license.features.join(", "));
    let state = match license.state(now()) {
        LicenseState::NotYetValid => "not yet valid".to_owned(),
        LicenseState::Active { expires_in } => format!("active, {} days left", expires_in / DAY),
        LicenseState::Grace { ends_in } => format!("in grace, {} days left", ends_in / DAY),
        LicenseState::Expired => "expired".to_owned(),
    };
    println!("state:     {state}");
    Ok(())
}

fn sign_update(args: &[String]) -> Result<(), String> {
    let key =
        std::fs::read(need(args, "--key")?).map_err(|e| format!("cannot read the key: {e}"))?;
    let issuer = Issuer::from_pkcs8(&key).map_err(|e| e.to_string())?;
    let path = need(args, "--in")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{path} is not JSON: {e}"))?;
    let object = value
        .as_object_mut()
        .ok_or("the manifest must be a JSON object")?;
    object.insert("version".into(), 1.into());
    object.insert("kid".into(), need(args, "--key-id")?.into());
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|e| format!("bad manifest: {e}"))?;
    println!(
        "{}",
        issuer.sign_manifest(&manifest).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn inspect_update(args: &[String]) -> Result<(), String> {
    let file = args.get(1).ok_or("inspect-update needs a manifest file")?;
    let text = std::fs::read_to_string(file).map_err(|e| format!("cannot read {file}: {e}"))?;
    let keys_text = std::fs::read_to_string(need(args, "--keys")?)
        .map_err(|e| format!("cannot read the key list: {e}"))?;
    let keys = TrustedKeys::parse(&keys_text).map_err(|e| e.to_string())?;
    let manifest = Manifest::verify(&text, &keys).map_err(|e| e.to_string())?;
    println!("release:   {}", manifest.release);
    println!("published: {}", manifest.published_at);
    for a in &manifest.artifacts {
        println!(
            "{:<16} {} bytes  {}  {}",
            a.platform, a.bytes, a.sha256, a.url
        );
    }
    Ok(())
}

fn read_keys(args: &[String]) -> Result<TrustedKeys, String> {
    let keys_text = std::fs::read_to_string(need(args, "--keys")?)
        .map_err(|e| format!("cannot read the key list: {e}"))?;
    TrustedKeys::parse(&keys_text).map_err(|e| e.to_string())
}

fn sign_bundle(args: &[String]) -> Result<(), String> {
    let dir = std::path::Path::new(args.get(1).ok_or("sign-bundle needs a bundle folder")?);
    let key =
        std::fs::read(need(args, "--key")?).map_err(|e| format!("cannot read the key: {e}"))?;
    let issuer = Issuer::from_pkcs8(&key).map_err(|e| e.to_string())?;
    let sums = bundle::sums_for_dir(dir).map_err(|e| e.to_string())?;
    let sig = issuer.sign_bundle_sums(&sums).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(bundle::SUMS_FILE), sums.as_bytes())
        .map_err(|e| format!("cannot write {}: {e}", bundle::SUMS_FILE))?;
    std::fs::write(dir.join(bundle::SIG_FILE), sig)
        .map_err(|e| format!("cannot write {}: {e}", bundle::SIG_FILE))?;
    println!("signed {} files", sums.lines().count());
    Ok(())
}

fn verify_bundle(args: &[String]) -> Result<(), String> {
    let dir = std::path::Path::new(args.get(1).ok_or("verify-bundle needs a bundle folder")?);
    let keys = read_keys(args)?;
    let report = bundle::verify_bundle_dir(dir, &keys).map_err(|e| e.to_string())?;
    println!("release:   {}", report.release);
    println!("key:       {}", report.key_id);
    println!("files:     {}", report.files);
    println!("bundle ok");
    Ok(())
}

fn public_pem(args: &[String]) -> Result<(), String> {
    let keys = read_keys(args)?;
    let pem =
        bundle::public_key_pem(&keys, need(args, "--key-id")?).ok_or("no key with that ID")?;
    print!("{pem}");
    Ok(())
}
