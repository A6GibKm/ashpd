use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

/// The application ID.
///
/// See <https://developer.gnome.org/documentation/tutorials/application-id.html>.
#[derive(Debug, Serialize, Deserialize, Type, PartialEq, Eq, Hash)]
pub struct AppID(String);

unsafe impl Send for AppID {}
unsafe impl Sync for AppID {}

impl From<&str> for AppID {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for AppID {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl AsRef<str> for AppID {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl std::fmt::Display for AppID {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// The ID of a file in the document store.
#[derive(Debug, Serialize, Deserialize, Type, PartialEq, Eq, Hash)]
pub struct DocumentID(String);

impl From<&str> for DocumentID {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for DocumentID {
    fn from(value: String) -> Self {
        Self(value)
    }
}
