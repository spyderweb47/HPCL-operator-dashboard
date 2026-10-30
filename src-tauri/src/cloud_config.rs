//! Where the cloud MongoDB address comes from. Nothing is built into the app: the operator
//! enters it in Settings, which saves it to `cloud.json` in the app's config folder. For
//! development, the `HPCL_MONGODB_URI` environment variable is used when no file exists.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::store::R;

pub const DEFAULT_DATABASE: &str = "hpcl_dealership";
const FILE: &str = "cloud.json";

#[derive(Clone, Debug, PartialEq)]
pub struct CloudConfig {
    pub uri: String,
    pub database: String,
    /// "settings" | "environment"
    pub source: &'static str,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    mongodb_uri: String,
    #[serde(default)]
    database: Option<String>,
}

pub fn load(dir: &Path) -> Option<CloudConfig> {
    let file = std::fs::read_to_string(dir.join(FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<ConfigFile>(&s).ok())
        .filter(|f| !f.mongodb_uri.trim().is_empty());
    if let Some(f) = file {
        let uri = f.mongodb_uri.trim().to_string();
        return Some(CloudConfig { database: pick_database(f.database, &uri), uri, source: "settings" });
    }
    let uri = std::env::var("HPCL_MONGODB_URI").ok().filter(|u| !u.trim().is_empty())?;
    let uri = uri.trim().to_string();
    Some(CloudConfig { database: pick_database(std::env::var("HPCL_MONGODB_DB").ok(), &uri), uri, source: "environment" })
}

pub fn save(dir: &Path, cfg: &CloudConfig) -> R<()> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let body = serde_json::to_string_pretty(&ConfigFile { mongodb_uri: cfg.uri.clone(), database: Some(cfg.database.clone()) })
        .map_err(|e| e.to_string())?;
    std::fs::write(dir.join(FILE), body).map_err(|e| format!("Cannot save the cloud settings: {e}"))
}

pub fn clear(dir: &Path) -> R<()> {
    match std::fs::remove_file(dir.join(FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("Cannot remove the cloud settings: {e}")),
        _ => Ok(()),
    }
}

fn pick_database(explicit: Option<String>, uri: &str) -> String {
    explicit
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .or_else(|| split(uri).ok().and_then(|p| database_in_path(&p.rest)))
        .unwrap_or_else(|| DEFAULT_DATABASE.into())
}

/// `host/<db>?opts` -> `<db>`
fn database_in_path(rest: &str) -> Option<String> {
    let path = rest.split_once('/')?.1;
    let name = path.split('?').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// A connection string split into its parts. `user`/`pass` stay percent-encoded, as in the URI.
#[derive(Debug)]
struct Parts {
    scheme: &'static str,
    user: Option<String>,
    pass: Option<String>,
    /// hosts + optional `/db` + `?options`
    rest: String,
}

fn split(uri: &str) -> R<Parts> {
    let uri = uri.trim();
    let (scheme, rest) = if let Some(r) = uri.strip_prefix("mongodb+srv://") {
        ("mongodb+srv://", r)
    } else if let Some(r) = uri.strip_prefix("mongodb://") {
        ("mongodb://", r)
    } else {
        return Err("The connection string must start with mongodb+srv:// or mongodb://".into());
    };
    let (userinfo, rest) = match rest.rfind('@') {
        Some(i) => (Some(&rest[..i]), &rest[i + 1..]),
        None => (None, rest),
    };
    if host_of(rest).is_empty() {
        return Err("The connection string has no server address after the @.".into());
    }
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let (user, pass) = match userinfo.map(|u| u.split_once(':').unwrap_or((u, ""))) {
        Some((u, p)) => (some(u), some(p)),
        None => (None, None),
    };
    Ok(Parts { scheme, user, pass, rest: rest.to_string() })
}

fn host_of(rest: &str) -> &str {
    rest.split(['/', '?']).next().unwrap_or(rest)
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// What Settings submits.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudInput {
    pub connection_string: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub database: String,
}

/// Build the address to use from the form. The username/password boxes win over what's in the
/// pasted string; Atlas's `<db_password>` placeholder counts as "no password"; a blank password
/// keeps the saved one when the cluster and user haven't changed.
pub fn compose(input: &CloudInput, saved: Option<&CloudConfig>) -> R<CloudConfig> {
    let p = split(&input.connection_string)?;
    let user = match input.username.trim() {
        "" => p.user.clone(),
        u => Some(encode(u)),
    };
    let is_placeholder = |v: &str| decode(v).starts_with('<') && decode(v).ends_with('>');
    let mut pass = if !input.password.is_empty() {
        Some(encode(&input.password))
    } else {
        p.pass.clone().filter(|v| !is_placeholder(v))
    };
    if let (Some(u), None) = (&user, &pass) {
        if let Some(sp) = saved.and_then(|s| split(&s.uri).ok()) {
            if sp.user.as_deref() == Some(u.as_str()) && host_of(&sp.rest).eq_ignore_ascii_case(host_of(&p.rest)) {
                pass = sp.pass;
            }
        }
        if pass.is_none() {
            return Err("Enter the database user's password.".into());
        }
    }
    let userinfo = match (&user, &pass) {
        (Some(u), Some(pw)) => format!("{u}:{pw}@"),
        (Some(u), None) => format!("{u}@"),
        _ => String::new(),
    };
    let uri = format!("{}{}{}", p.scheme, userinfo, p.rest);
    let database = pick_database(Some(input.database.clone()), &uri);
    if database.len() > 60 || database.chars().any(|c| matches!(c, ' ' | '.' | '$' | '/' | '\\' | '"')) {
        return Err("Database name can't contain spaces or . $ / \\ \" characters.".into());
    }
    Ok(CloudConfig { uri, database, source: "settings" })
}

/// Which cluster + database the local rows were last uploaded to. When it changes, everything
/// on this PC is uploaded again to the new place.
pub fn target_key(cfg: &CloudConfig) -> String {
    let host = split(&cfg.uri).map(|p| host_of(&p.rest).to_lowercase()).unwrap_or_default();
    format!("{host}/{}", cfg.database)
}

/// Safe to show on screen: never includes the password.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSettingsDTO {
    pub configured: bool,
    pub source: Option<&'static str>,
    /// The saved address without username/password.
    pub connection_string: String,
    pub username: String,
    pub has_password: bool,
    pub host: String,
    pub database: String,
}

pub fn describe(cfg: Option<&CloudConfig>) -> CloudSettingsDTO {
    let parts = cfg.and_then(|c| split(&c.uri).ok());
    CloudSettingsDTO {
        configured: cfg.is_some(),
        source: cfg.map(|c| c.source),
        connection_string: parts.as_ref().map(|p| format!("{}{}", p.scheme, p.rest)).unwrap_or_default(),
        username: parts.as_ref().and_then(|p| p.user.as_deref()).map(decode).unwrap_or_default(),
        has_password: parts.as_ref().is_some_and(|p| p.pass.is_some()),
        host: parts.as_ref().map(|p| host_of(&p.rest).to_string()).unwrap_or_default(),
        database: cfg.map(|c| c.database.clone()).unwrap_or_else(|| DEFAULT_DATABASE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(cs: &str, user: &str, pass: &str, db: &str) -> CloudInput {
        CloudInput { connection_string: cs.into(), username: user.into(), password: pass.into(), database: db.into() }
    }

    #[test]
    fn compose_from_atlas_template() {
        let atlas = "mongodb+srv://shop_user:<db_password>@cluster0.ab12.mongodb.net/?retryWrites=true&w=majority";
        assert!(compose(&input(atlas, "", "", ""), None).unwrap_err().contains("password"));
        let c = compose(&input(atlas, "", "p@ss:w/rd", ""), None).unwrap();
        assert_eq!(c.uri, "mongodb+srv://shop_user:p%40ss%3Aw%2Frd@cluster0.ab12.mongodb.net/?retryWrites=true&w=majority");
        assert_eq!(c.database, DEFAULT_DATABASE);
        let d = describe(Some(&c));
        assert_eq!((d.username.as_str(), d.has_password, d.host.as_str()), ("shop_user", true, "cluster0.ab12.mongodb.net"));
        assert!(!d.connection_string.contains("shop_user") && !d.connection_string.contains("p%40ss"));
    }

    #[test]
    fn blank_password_keeps_saved_one_only_for_same_cluster_and_user() {
        let saved = compose(&input("mongodb+srv://u:secret@a.mongodb.net/", "", "", ""), None).unwrap();
        // Change only the database: password kept.
        let c = compose(&input("mongodb+srv://a.mongodb.net/", "u", "", "other_db"), Some(&saved)).unwrap();
        assert_eq!((c.uri.as_str(), c.database.as_str()), ("mongodb+srv://u:secret@a.mongodb.net/", "other_db"));
        assert_ne!(target_key(&saved), target_key(&c));
        // Different cluster: must type the password.
        assert!(compose(&input("mongodb+srv://b.mongodb.net/", "u", "", ""), Some(&saved)).is_err());
        // Full string pasted with its own credentials and database in the path.
        let full = compose(&input("mongodb://x:y@localhost:27017/shopdb", "", "", ""), Some(&saved)).unwrap();
        assert_eq!((full.uri.as_str(), full.database.as_str()), ("mongodb://x:y@localhost:27017/shopdb", "shopdb"));
        // No credentials at all (local server) is allowed.
        assert!(compose(&input("mongodb://127.0.0.1:27017", "", "", ""), None).is_ok());
        assert!(compose(&input("http://nope", "", "", ""), None).is_err());
        assert!(compose(&input("mongodb://h/", "", "", "bad name"), None).is_err());
    }
}
