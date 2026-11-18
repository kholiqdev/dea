use std::fmt;

use dea_project::SecretKind;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use zeroize::Zeroizing;

use crate::VaultError;

/// A secret value. `Debug` never shows it.
pub enum SecretData {
    /// Token, password, SSH key or API key.
    Text(SecretString),
    OAuth(OAuthToken),
}

/// An OAuth token with what is needed to refresh it.
pub struct OAuthToken {
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
    pub expires_at: Option<OffsetDateTime>,
}

impl fmt::Debug for SecretData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretData(***)")
    }
}

impl fmt::Debug for OAuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthToken")
            .field("access_token", &"***")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "***"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// How an OAuth token is stored: JSON with camelCase keys.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredOAuth<'a> {
    access_token: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_token: Option<&'a str>,
    #[serde(
        with = "time::serde::rfc3339::option",
        skip_serializing_if = "Option::is_none"
    )]
    expires_at: Option<OffsetDateTime>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadedOAuth {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    expires_at: Option<OffsetDateTime>,
}

impl SecretData {
    /// The text a request or connection uses: the access token for OAuth.
    pub fn value_for_use(&self) -> &str {
        match self {
            SecretData::Text(value) => value.expose_secret(),
            SecretData::OAuth(token) => token.access_token.expose_secret(),
        }
    }

    /// When the secret stops working, if it is known.
    pub fn expires_at(&self) -> Option<OffsetDateTime> {
        match self {
            SecretData::Text(_) => None,
            SecretData::OAuth(token) => token.expires_at,
        }
    }

    /// The text kept in the secret store.
    pub fn encode(&self) -> Zeroizing<String> {
        match self {
            SecretData::Text(value) => Zeroizing::new(value.expose_secret().to_string()),
            SecretData::OAuth(token) => {
                let stored = StoredOAuth {
                    access_token: token.access_token.expose_secret(),
                    refresh_token: token
                        .refresh_token
                        .as_ref()
                        .map(ExposeSecret::expose_secret),
                    expires_at: token.expires_at,
                };
                // Serializing strings and a timestamp cannot fail.
                Zeroizing::new(serde_json::to_string(&stored).unwrap_or_default())
            }
        }
    }

    /// Reads text written by [`SecretData::encode`] for a secret of `kind`.
    pub fn decode(kind: SecretKind, text: &str) -> Result<SecretData, VaultError> {
        if kind != SecretKind::OauthToken {
            return Ok(SecretData::Text(SecretString::from(text)));
        }
        // The parser's own message may quote the input, so it is dropped.
        let loaded: LoadedOAuth = serde_json::from_str(text)
            .map_err(|_| VaultError::Corrupt("a stored OAuth token is not valid".into()))?;
        let LoadedOAuth {
            access_token,
            refresh_token,
            expires_at,
        } = loaded;
        Ok(SecretData::OAuth(OAuthToken {
            access_token: SecretString::from(access_token),
            refresh_token: refresh_token.map(SecretString::from),
            expires_at,
        }))
    }
}
