use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// API key wrapper: cannot be Display'd, Debug prints `<redacted>`.
#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file {0} is readable by group/other; run: chmod 600 {0}")]
    Permissions(PathBuf),
    #[error("cannot read config file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid TOML in {path}: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("profile \"{0}\" not found in config file")]
    UnknownProfile(String),
    #[error(
        "missing credentials: {0}; set them in ~/.config/ncheap/config.toml or via NCHEAP_* environment variables"
    )]
    Missing(String),
    #[error("invalid value for {0} (expected true or false)")]
    Invalid(String),
}

#[derive(Debug, Default, Deserialize)]
pub struct ConfigFile {
    pub default_profile: Option<String>,
    #[serde(default)]
    pub profile: HashMap<String, ProfileFile>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProfileFile {
    pub api_user: Option<String>,
    pub api_key: Option<Secret>,
    pub username: Option<String>,
    pub client_ip: Option<String>,
    pub sandbox: Option<bool>,
}

/// Fully resolved credentials for one environment.
#[derive(Clone, Debug)]
pub struct Profile {
    pub name: String,
    pub api_user: String,
    pub api_key: Secret,
    pub username: String,
    pub client_ip: String,
    pub sandbox: bool,
}

impl Profile {
    pub fn endpoint(&self) -> &'static str {
        if self.sandbox {
            "https://api.sandbox.namecheap.com/xml.response"
        } else {
            "https://api.namecheap.com/xml.response"
        }
    }
}

/// The only config location read. Deliberately no --config flag and no
/// repo-relative lookup: a key file inside a working tree is a commit hazard.
pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ncheap").join("config.toml"))
}

pub fn load(profile_flag: Option<&str>) -> Result<Profile, ConfigError> {
    let file = match config_path() {
        Some(p) if p.exists() => Some(read_config_file(&p)?),
        _ => None,
    };
    resolve(file, profile_flag, &|k| std::env::var(k).ok())
}

fn read_config_file(path: &Path) -> Result<ConfigFile, ConfigError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(path).map_err(|e| ConfigError::Read {
            path: path.to_owned(),
            source: e,
        })?;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(ConfigError::Permissions(path.to_owned()));
        }
    }
    let raw = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
        path: path.to_owned(),
        source: e,
    })?;
    toml::from_str(&raw).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        source: Box::new(e),
    })
}

/// Precedence: env vars > profile from config file. Pure-env operation (no
/// config file at all) is supported for agents/CI that inject credentials.
pub fn resolve(
    file: Option<ConfigFile>,
    profile_flag: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Profile, ConfigError> {
    let file = file.unwrap_or_default();
    let explicit =
        profile_flag.is_some() || env("NCHEAP_PROFILE").is_some() || file.default_profile.is_some();
    let name = profile_flag
        .map(str::to_owned)
        .or_else(|| env("NCHEAP_PROFILE"))
        .or_else(|| file.default_profile.clone())
        .unwrap_or_else(|| "production".to_owned());

    let base = match file.profile.get(&name) {
        Some(p) => p.clone(),
        None if explicit && !file.profile.is_empty() => {
            return Err(ConfigError::UnknownProfile(name));
        }
        None => ProfileFile::default(),
    };

    let sandbox = match env("NCHEAP_SANDBOX") {
        Some(v) => parse_bool(&v).ok_or_else(|| ConfigError::Invalid("NCHEAP_SANDBOX".into()))?,
        None => base.sandbox.unwrap_or(false),
    };
    let api_user = env("NCHEAP_API_USER").or(base.api_user);
    let api_key = env("NCHEAP_API_KEY").map(Secret::new).or(base.api_key);
    let client_ip = env("NCHEAP_CLIENT_IP").or(base.client_ip);
    let username = env("NCHEAP_USERNAME").or(base.username);

    let mut missing = Vec::new();
    if api_user.is_none() {
        missing.push("api_user (NCHEAP_API_USER)");
    }
    if api_key.is_none() {
        missing.push("api_key (NCHEAP_API_KEY)");
    }
    if client_ip.is_none() {
        missing.push("client_ip (NCHEAP_CLIENT_IP)");
    }
    if !missing.is_empty() {
        return Err(ConfigError::Missing(missing.join(", ")));
    }

    let api_user = api_user.expect("checked above");
    Ok(Profile {
        username: username.unwrap_or_else(|| api_user.clone()),
        name,
        api_user,
        api_key: api_key.expect("checked above"),
        client_ip: client_ip.expect("checked above"),
        sandbox,
    })
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| {
            pairs
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    fn sample_file() -> ConfigFile {
        toml::from_str(
            r#"
            default_profile = "production"

            [profile.production]
            api_user = "fileuser"
            api_key = "filekey"
            client_ip = "192.0.2.10"

            [profile.sandbox]
            api_user = "sbuser"
            api_key = "sbkey"
            client_ip = "192.0.2.10"
            sandbox = true
            "#,
        )
        .unwrap()
    }

    #[test]
    fn file_profile_resolves_and_username_defaults_to_api_user() {
        let p = resolve(Some(sample_file()), None, &env_of(&[])).unwrap();
        assert_eq!(p.name, "production");
        assert_eq!(p.api_user, "fileuser");
        assert_eq!(p.username, "fileuser");
        assert!(!p.sandbox);
        assert_eq!(p.endpoint(), "https://api.namecheap.com/xml.response");
    }

    #[test]
    fn env_overrides_file() {
        let env = env_of(&[("NCHEAP_API_KEY", "envkey"), ("NCHEAP_USERNAME", "other")]);
        let p = resolve(Some(sample_file()), None, &env).unwrap();
        assert_eq!(p.api_key.expose(), "envkey");
        assert_eq!(p.username, "other");
        assert_eq!(p.api_user, "fileuser");
    }

    #[test]
    fn profile_flag_selects_sandbox() {
        let p = resolve(Some(sample_file()), Some("sandbox"), &env_of(&[])).unwrap();
        assert!(p.sandbox);
        assert_eq!(
            p.endpoint(),
            "https://api.sandbox.namecheap.com/xml.response"
        );
    }

    #[test]
    fn unknown_explicit_profile_errors() {
        let err = resolve(Some(sample_file()), Some("staging"), &env_of(&[])).unwrap_err();
        assert!(matches!(err, ConfigError::UnknownProfile(name) if name == "staging"));
    }

    #[test]
    fn pure_env_operation_without_file() {
        let env = env_of(&[
            ("NCHEAP_API_USER", "envuser"),
            ("NCHEAP_API_KEY", "envkey"),
            ("NCHEAP_CLIENT_IP", "192.0.2.20"),
        ]);
        let p = resolve(None, None, &env).unwrap();
        assert_eq!(p.api_user, "envuser");
        assert_eq!(p.username, "envuser");
    }

    #[test]
    fn missing_fields_are_listed() {
        let err = resolve(None, None, &env_of(&[("NCHEAP_API_USER", "u")])).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("api_key"));
        assert!(msg.contains("client_ip"));
        assert!(!msg.contains("api_user ("));
    }

    #[test]
    fn secret_debug_is_redacted() {
        let s = Secret::new("supersecret".into());
        assert_eq!(format!("{s:?}"), "<redacted>");
    }
}
