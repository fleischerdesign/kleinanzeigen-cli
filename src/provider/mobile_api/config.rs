//! Provider configuration and the app-level Basic credential.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use url::Url;

use crate::error::{Error, Result};

/// Default mobile API host.
pub const DEFAULT_BASE_URL: &str = "https://api.kleinanzeigen.de";

/// Default app version sent in the provider headers.
///
/// This is a compatibility value, not a contract. The mobile API is private and
/// the app version can be overridden with `KLEINANZEIGEN_APP_VERSION`.
pub const DEFAULT_APP_VERSION: &str = "2026.25.0";

/// The app-level Basic credential.
///
/// This is the credential embedded in the mobile app, not a user account
/// password. It is supplied by the operator and is never part of this
/// repository. The `Debug` implementation redacts the password.
#[derive(Clone)]
pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    /// Creates a credential after checking it can be sent as a header.
    pub fn new(username: impl Into<String>, password: impl Into<String>) -> Result<Self> {
        let username: String = username.into();
        let password: String = password.into();

        if username.is_empty() {
            return Err(Error::Input(
                "basic-auth username must not be empty".to_owned(),
            ));
        }
        if password.is_empty() {
            return Err(Error::Input(
                "basic-auth password must not be empty".to_owned(),
            ));
        }
        if username.contains(':') {
            return Err(Error::Input(
                "basic-auth username must not contain a colon".to_owned(),
            ));
        }
        if username.chars().any(char::is_control) || password.chars().any(char::is_control) {
            return Err(Error::Input(
                "basic-auth values must not contain control characters".to_owned(),
            ));
        }

        Ok(Self { username, password })
    }

    /// Returns the value for the `Authorization` header.
    pub fn authorization_header(&self) -> String {
        let encoded = STANDARD.encode(format!("{}:{}", self.username, self.password));
        format!("Basic {encoded}")
    }

    /// Returns the username. The password is deliberately not exposed.
    pub fn username(&self) -> &str {
        &self.username
    }
}

impl fmt::Debug for BasicAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BasicAuth")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// Configuration for the mobile API provider.
#[derive(Debug, Clone)]
pub struct AppApiSettings {
    base_url: Url,
    app_version: String,
    auth: Option<BasicAuth>,
}

impl AppApiSettings {
    /// Reads settings from the environment.
    ///
    /// `KLEINANZEIGEN_BASIC_USER` and `KLEINANZEIGEN_BASIC_PW` are optional as a
    /// pair. `KLEINANZEIGEN_BASE_URL` and `KLEINANZEIGEN_APP_VERSION` override
    /// the defaults, mainly for testing and compatibility.
    pub fn from_env() -> Result<Self> {
        let base_url =
            optional_env("KLEINANZEIGEN_BASE_URL")?.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let app_version = optional_env("KLEINANZEIGEN_APP_VERSION")?
            .unwrap_or_else(|| DEFAULT_APP_VERSION.to_owned());

        let username = optional_env("KLEINANZEIGEN_BASIC_USER")?;
        let password = optional_env("KLEINANZEIGEN_BASIC_PW")?;
        let auth = match (username, password) {
            (None, None) => None,
            (Some(username), Some(password)) => Some(BasicAuth::new(username, password)?),
            _ => {
                return Err(Error::Input(
                    "set both KLEINANZEIGEN_BASIC_USER and KLEINANZEIGEN_BASIC_PW, or neither"
                        .to_owned(),
                ));
            }
        };

        Self::new(base_url, app_version, auth)
    }

    /// Creates settings from explicit values.
    pub fn new(
        base_url: impl AsRef<str>,
        app_version: impl Into<String>,
        auth: Option<BasicAuth>,
    ) -> Result<Self> {
        let mut base_url = Url::parse(base_url.as_ref())
            .map_err(|error| Error::Input(format!("invalid base URL: {error}")))?;

        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err(Error::Input(
                "base URL must not contain a query or fragment".to_owned(),
            ));
        }
        if !base_url.username().is_empty() || base_url.password().is_some() {
            return Err(Error::Input(
                "base URL must not contain credentials".to_owned(),
            ));
        }
        if base_url.host_str().is_none() {
            return Err(Error::Input("base URL must have a host".to_owned()));
        }

        match base_url.scheme() {
            "https" => {}
            "http" if is_loopback(&base_url) => {}
            _ => {
                return Err(Error::Input(
                    "base URL must use https; http is allowed only for loopback tests".to_owned(),
                ));
            }
        }

        if !base_url.path().ends_with('/') {
            let path = format!("{}/", base_url.path());
            base_url.set_path(&path);
        }

        let app_version: String = app_version.into();
        if app_version.is_empty()
            || !app_version.is_ascii()
            || app_version.chars().any(char::is_control)
        {
            return Err(Error::Input(
                "app version must be a non-empty ASCII value".to_owned(),
            ));
        }

        Ok(Self {
            base_url,
            app_version,
            auth,
        })
    }

    /// Returns the normalized base URL, always ending in a slash.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Returns the app version used in request headers.
    pub fn app_version(&self) -> &str {
        &self.app_version
    }

    /// Returns the configured app credential, if any.
    pub fn auth(&self) -> Option<&BasicAuth> {
        self.auth.as_ref()
    }

    /// Returns the app credential or a configuration error.
    pub fn require_auth(&self) -> Result<&BasicAuth> {
        self.auth.as_ref().ok_or_else(|| {
            Error::Input(
                "provider credentials are not configured; set KLEINANZEIGEN_BASIC_USER and \
                 KLEINANZEIGEN_BASIC_PW"
                    .to_owned(),
            )
        })
    }

    /// Returns whether both the endpoint and the app credential are configured.
    pub fn is_configured(&self) -> bool {
        self.auth.is_some()
    }
}

fn optional_env(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::Input(format!("{name} must be valid UTF-8")))
        }
    }
}

fn is_loopback(url: &Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "::1" | "[::1]")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_is_normalized_to_a_directory() {
        let settings = AppApiSettings::new("https://api.example.invalid", "1.2.3", None)
            .expect("valid settings");
        assert_eq!(settings.base_url().as_str(), "https://api.example.invalid/");
    }

    #[test]
    fn http_is_rejected_outside_loopback() {
        assert!(AppApiSettings::new("http://api.example.invalid", "1.2.3", None).is_err());
        assert!(AppApiSettings::new("http://localhost:8080", "1.2.3", None).is_ok());
        assert!(AppApiSettings::new("http://127.0.0.1:8080", "1.2.3", None).is_ok());
    }

    #[test]
    fn base_url_rejects_queries_credentials_and_bad_versions() {
        assert!(AppApiSettings::new("https://example.invalid/?q=1", "1.2.3", None).is_err());
        assert!(AppApiSettings::new("https://user:pass@example.invalid", "1.2.3", None).is_err());
        assert!(AppApiSettings::new("https://example.invalid", "1.2.3\r\n", None).is_err());
    }

    #[test]
    fn basic_auth_header_is_standard_base64() {
        let auth = BasicAuth::new("test", "secret").expect("valid auth");
        assert_eq!(auth.authorization_header(), "Basic dGVzdDpzZWNyZXQ=");
    }

    #[test]
    fn basic_auth_debug_redacts_the_password() {
        let auth = BasicAuth::new("test", "secret").expect("valid auth");
        let debug = format!("{auth:?}");
        assert!(debug.contains("test"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn basic_auth_rejects_colons_and_control_characters() {
        assert!(BasicAuth::new("te:st", "secret").is_err());
        assert!(BasicAuth::new("test", "sec\nret").is_err());
        assert!(BasicAuth::new("", "secret").is_err());
    }

    #[test]
    fn missing_auth_is_reported_as_configuration_input() {
        let settings =
            AppApiSettings::new("https://example.invalid", "1.2.3", None).expect("valid settings");
        assert!(!settings.is_configured());
        assert!(settings.require_auth().is_err());
    }
}
