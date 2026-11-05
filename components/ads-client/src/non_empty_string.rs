/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use serde::{Deserialize, Deserializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NonEmptyString(String);

impl NonEmptyString {
    pub(crate) fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()).then_some(Self(value))
    }

    pub(crate) fn deserialize_option<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Self>, D::Error> {
        Ok(Option::<String>::deserialize(deserializer)?.and_then(Self::new))
    }
}

impl AsRef<str> for NonEmptyString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<NonEmptyString> for String {
    fn from(value: NonEmptyString) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_only_the_empty_string() {
        assert_eq!(NonEmptyString::new(""), None);
        assert_eq!(NonEmptyString::new(" ").unwrap().as_ref(), " ");
    }
}
