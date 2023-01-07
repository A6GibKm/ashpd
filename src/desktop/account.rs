//! Access to the current logged user information such as the id, name
//! or their avatar uri.
//!
//! Wrapper of the DBus interface: [`org.freedesktop.portal.Account`](https://flatpak.github.io/xdg-desktop-portal/index.html#gdbus-org.freedesktop.portal.Account).
//!
//! ### Examples
//!
//! ```rust, no_run
//! use ashpd::desktop::account::UserInformationRequest;
//!
//! async fn run() -> ashpd::Result<()> {
//!     let response = UserInformationRequest::default()
//!         .reason("App would like to access user information")
//!         .build()
//!         .await?
//!         .response()?;
//!
//!     println!("Name: {:?}", response.name());
//!     println!("ID: {:?}", response.id());
//!
//!     Ok(())
//! }
//! ```

use zbus::zvariant::{DeserializeDict, SerializeDict, Type};

use super::HandleToken;
use crate::{desktop::request::Request, proxy::Proxy, Error, WindowIdentifier};

#[derive(SerializeDict, Type, Debug, Default)]
#[zvariant(signature = "dict")]
struct UserInformationOptions {
    handle_token: HandleToken,
    reason: Option<String>,
}

#[derive(Debug, Default, SerializeDict, DeserializeDict, Type)]
/// The response of a [`UserInformationRequest`] request.
#[zvariant(signature = "dict")]
pub struct UserInformation {
    id: Option<String>,
    name: Option<String>,
    image: Option<url::Url>,
}

impl UserInformation {
    #[cfg(feature = "backend")]
    pub fn new(id: &str, name: &str, image: url::Url) -> Self {
        Self {
            id: Some(id.to_owned()),
            name: Some(name.to_owned()),
            image: Some(image),
        }
    }

    /// User identifier.
    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// User name.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// User image uri.
    pub fn image(&self) -> Option<&url::Url> {
        self.image.as_ref()
    }

    /// Creates a new builder-pattern struct instance to construct
    /// [`UserInformation`].
    ///
    /// This method returns an instance of [`UserInformationRequest`].
    pub fn builder() -> UserInformationRequest {
        UserInformationRequest::default()
    }
}

struct AccountProxy<'a>(Proxy<'a>);

impl<'a> AccountProxy<'a> {
    pub async fn new() -> Result<AccountProxy<'a>, Error> {
        let proxy = Proxy::new_desktop("org.freedesktop.portal.Account").await?;
        Ok(Self(proxy))
    }

    pub async fn user_information(
        &self,
        identifier: &WindowIdentifier,
        options: UserInformationOptions,
    ) -> Result<Request<UserInformation>, Error> {
        self.0
            .request(
                &options.handle_token,
                "GetUserInformation",
                (&identifier, &options),
            )
            .await
    }
}

#[doc(alias = "xdp_portal_get_user_information")]
#[doc(alias = "org.freedesktop.portal.Account")]
#[derive(Debug, Default)]
/// A [builder-pattern] type to construct [`UserInformation`].
///
/// [builder-pattern]: https://doc.rust-lang.org/1.0.0/style/ownership/builders.html
pub struct UserInformationRequest {
    options: UserInformationOptions,
    identifier: WindowIdentifier,
}

impl UserInformationRequest {
    #[must_use]
    /// Sets a user-visible reason for the request.
    pub fn reason<'a>(mut self, reason: impl Into<Option<&'a str>>) -> Self {
        self.options.reason = reason.into().map(ToOwned::to_owned);
        self
    }

    #[must_use]
    /// Sets a window identifier.
    pub fn identifier(mut self, identifier: impl Into<Option<WindowIdentifier>>) -> Self {
        self.identifier = identifier.into().unwrap_or_default();
        self
    }

    /// Build the [`UserInformation`].
    pub async fn build(self) -> Result<Request<UserInformation>, Error> {
        let proxy = AccountProxy::new().await?;
        proxy.user_information(&self.identifier, self.options).await
    }
}
