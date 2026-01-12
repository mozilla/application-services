use crate::Jwk;
use crate::error::{JwCryptoError, Result};
use std::sync::Arc;

#[cfg(feature = "keydb")]
use futures::executor::block_on;

#[cfg(feature = "keydb")]
use async_trait::async_trait;

#[cfg(feature = "keydb")]
use nss::assert_initialized as assert_nss_initialized;
#[cfg(feature = "keydb")]
use nss::pk11::sym_key::{
    authenticate_with_primary_password, authentication_with_primary_password_is_needed,
    get_or_create_aes256_key,
};

/// Identifier for the as key, under which the key is stored in NSS.
#[cfg(feature = "keydb")]
static KEY_NAME: &str = "as-key";

/// Consumers can implement the KeyManager in combination with the ManagedEncryptorDecryptor to hand
/// over the encryption key whenever encryption or decryption happens.
pub trait KeyManager: Send + Sync {
    fn get_key(&self) -> Result<Vec<u8>>;
}

/// Last but not least we provide a StaticKeyManager, which can be
/// used in cases where there is a single key during runtime, for example in tests.
pub struct StaticKeyManager {
    key: String,
}

impl StaticKeyManager {
    pub fn new(key: String) -> Self {
        Self { key }
    }
}

impl KeyManager for StaticKeyManager {
    fn get_key(&self) -> Result<Vec<u8>> {
        Ok(self.key.as_bytes().into())
    }
}

/// `PrimaryPasswordAuthenticator` is used in conjunction with `NSSKeyManager` to provide the
/// primary password and the success or failure actions of the authentication process.
#[cfg(feature = "keydb")]
// #[uniffi::export(with_foreign)]
#[async_trait]
pub trait PrimaryPasswordAuthenticator: Send + Sync {
    /// Get a primary password for authentication, otherwise return the
    /// AuthenticationCancelled error to cancel the authentication process.
    async fn get_primary_password(&self) -> Result<String>;
    async fn on_authentication_success(&self) -> Result<()>;
    async fn on_authentication_failure(&self) -> Result<()>;
}

/// Use the `NSSKeyManager` to use NSS for key management.
///
/// NSS stores keys in `key4.db` within the profile and wraps the key with a key derived from the
/// primary password, if set. It defers to the provided `PrimaryPasswordAuthenticator`
/// implementation to handle user authentication.  Note that if no primary password is set, the
/// wrapping key is deterministically derived from an empty string.
///
/// Make sure to initialize NSS using `ensure_initialized_with_profile_dir` before creating a
/// NSSKeyManager.
///
/// # Examples
/// ```no_run
/// use async_trait::async_trait;
/// use jwcrypto::{
///   JwCryptoError,
///   keystore::{KeyManager, PrimaryPasswordAuthenticator, NSSKeyManager}
/// };
/// use std::sync::Arc;
///
/// struct MyPrimaryPasswordAuthenticator {}
///
/// #[async_trait]
/// impl PrimaryPasswordAuthenticator for MyPrimaryPasswordAuthenticator {
///     async fn get_primary_password(&self) -> Result<String, JwCryptoError> {
///         // Most likely, you would want to prompt for a password.
///         // let password = prompt_string("primary password").unwrap_or_default();
///         Ok("secret".to_string())
///     }
///
///     async fn on_authentication_success(&self) -> Result<(), JwCryptoError> {
///         println!("success");
///         Ok(())
///     }
///
///     async fn on_authentication_failure(&self) -> Result<(), JwCryptoError> {
///         println!("this did not work, please try again:");
///         Ok(())
///     }
/// }
/// let key_manager = NSSKeyManager::new(Arc::new(MyPrimaryPasswordAuthenticator {}));
/// assert_eq!(key_manager.get_key().unwrap().len(), 63);
/// ```
#[cfg(feature = "keydb")]
// #[derive(uniffi::Object)]
pub struct NSSKeyManager {
    primary_password_authenticator: Arc<dyn PrimaryPasswordAuthenticator>,
}

#[cfg(feature = "keydb")]
// #[uniffi::export]
impl NSSKeyManager {
    /// Initialize new `NSSKeyManager` with a given `PrimaryPasswordAuthenticator`.
    /// There must be a previous initializiation of NSS before initializing
    /// `NSSKeyManager`, otherwise this panics.
    // #[uniffi::constructor()]
    pub fn new(primary_password_authenticator: Arc<dyn PrimaryPasswordAuthenticator>) -> Self {
        assert_nss_initialized();
        Self {
            primary_password_authenticator,
        }
    }

    pub fn into_dyn_key_manager(self: Arc<Self>) -> Arc<dyn KeyManager> {
        self
    }
}

// wrapp `authentication_with_primary_password_is_needed` into an Result
#[cfg(feature = "keydb")]
fn api_authentication_with_primary_password_is_needed() -> Result<bool> {
    authentication_with_primary_password_is_needed().map_err(|e: nss::Error| {
        JwCryptoError::NSSAuthenticationError {
            reason: e.to_string(),
        }
    })
}

// wrapp `authenticate_with_primary_password` into an Result
#[cfg(feature = "keydb")]
fn api_authenticate_with_primary_password(primary_password: &str) -> Result<bool> {
    authenticate_with_primary_password(primary_password).map_err(|e: nss::Error| {
        JwCryptoError::NSSAuthenticationError {
            reason: e.to_string(),
        }
    })
}

#[cfg(feature = "keydb")]
impl KeyManager for NSSKeyManager {
    fn get_key(&self) -> Result<Vec<u8>> {
        if api_authentication_with_primary_password_is_needed()? {
            let primary_password =
                block_on(self.primary_password_authenticator.get_primary_password())?;
            let mut result = api_authenticate_with_primary_password(&primary_password)?;

            if result {
                block_on(
                    self.primary_password_authenticator
                        .on_authentication_success(),
                )?;
            } else {
                while !result {
                    block_on(
                        self.primary_password_authenticator
                            .on_authentication_failure(),
                    )?;

                    let primary_password =
                        block_on(self.primary_password_authenticator.get_primary_password())?;
                    result = api_authenticate_with_primary_password(&primary_password)?;
                }
                block_on(
                    self.primary_password_authenticator
                        .on_authentication_success(),
                )?;
            }
        }

        let key = get_or_create_aes256_key(KEY_NAME).map_err(|_| JwCryptoError::MissingKey)?;
        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(
            &mut bytes,
            &Jwk::new_direct_from_bytes(None, &key),
        )
        .unwrap();
        Ok(bytes)
    }
}

