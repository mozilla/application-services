/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// The `ErrorPath` struct is constructed in the default validator to be used
/// to derive where an error has been detected.
///
/// serde_yaml does not keep track of lines and columns so we need to be able to
/// indicate where an error takes place.
///
/// For reporting errors in the manifest on the command line, an error might have a path such as:
///
///  1. `features/messaging.messages['my-message'].MessageData#is-control` expects a boolean,
///  2. `features/homescreen.sections-enabled[HomeScreenSection#pocket]` expects a boolean
///  3. `objects/AwesomeBar.min-search-term`.
///
/// The path to an error is given by `&self.path`.
///
/// The defaults validation is exactly the same as the validation performed on the Feature Configuration
/// JSON in experimenter. Thus, `literals` is a `Vec<String>` of tokens found in JSON, which should in
/// almost all circumstances lead to the correct token being identified by line and column.
///
/// So the corresponding `literals` of a type mismatch error where an integer `1` is used instead
/// of a boolean would be:
///
///  1. `"messages"`, `{`, `"my-message"`, `"is-control"`, `1`
///  2. `"sections-enabled"`, `{`, `"pocket"`, `1`
///
/// `find_err(src: &str)` is used to find the line and column for the final `1` token.
/// Currently `find_err` exists in `inspector.rs`, but this should move (along with reduced visibility
/// of `literals`) in a future commit.
#[derive(Clone)]
pub(crate) struct ErrorPath {
    pub(crate) literals: Vec<String>,
    pub(crate) path: String,
}

impl ErrorPath {
    pub(crate) fn feature(name: &str) -> Self {
        Self {
            path: format!("features/{name}"),
            literals: Default::default(),
        }
    }

    pub(crate) fn object(name: &str) -> Self {
        Self {
            path: format!("objects/{name}"),
            literals: Default::default(),
        }
    }

    pub(crate) fn property(&self, prop_key: &str) -> Self {
        Self {
            path: format!("{}.{prop_key}", &self.path),
            literals: append_quoted(&self.literals, prop_key),
        }
    }

    pub(crate) fn enum_map_key(&self, enum_: &str, key: &str) -> Self {
        Self {
            path: format!("{}[{enum_}#{key}]", &self.path),
            literals: append(&self.literals, &["{".to_string(), format!("\"{key}\"")]),
        }
    }

    pub(crate) fn map_key(&self, key: &str) -> Self {
        Self {
            path: format!("{}['{key}']", &self.path),
            literals: append(&self.literals, &["{".to_string(), format!("\"{key}\"")]),
        }
    }

    pub(crate) fn array_index(&self, index: usize) -> Self {
        let mut literals = append1(&self.literals, "[");
        if index > 0 {
            literals.extend_from_slice(&[",".repeat(index)]);
        }
        Self {
            path: format!("{}[{index}]", &self.path),
            literals,
        }
    }

    pub(crate) fn object_value(&self, name: &str) -> Self {
        Self {
            path: format!("{}#{name}", &self.path),
            literals: append1(&self.literals, "{"),
        }
    }

    pub(crate) fn open_brace(&self) -> Self {
        Self {
            path: self.path.clone(),
            literals: append1(&self.literals, "{"),
        }
    }

    pub(crate) fn final_error(&self, hightlight: &str) -> Self {
        Self {
            path: self.path.clone(),
            literals: append1(&self.literals, hightlight),
        }
    }

    pub(crate) fn final_error_quoted(&self, hightlight: &str) -> Self {
        Self {
            path: self.path.clone(),
            literals: append_quoted(&self.literals, hightlight),
        }
    }
}

fn append(original: &[String], new: &[String]) -> Vec<String> {
    let mut clone = Vec::with_capacity(original.len() + new.len());
    clone.extend_from_slice(original);
    clone.extend_from_slice(new);
    clone
}

fn append1(original: &[String], new: &str) -> Vec<String> {
    let mut clone = Vec::with_capacity(original.len() + 1);
    clone.extend_from_slice(original);
    clone.push(new.to_string());
    clone
}

fn append_quoted(original: &[String], new: &str) -> Vec<String> {
    append1(original, &format!("\"{new}\""))
}

#[cfg(test)]
mod unit_tests {
    use super::ErrorPath;

    #[test]
    fn test_property() {
        let path = ErrorPath::feature("my-feature").property("my-property");
        assert_eq!("features/my-feature.my-property", &path.path);
        assert_eq!(&["\"my-property\""], path.literals.as_slice());

        let path = ErrorPath::object("MyObject").property("my-property");
        assert_eq!("objects/MyObject.my-property", &path.path);
        assert_eq!(&["\"my-property\""], path.literals.as_slice());
    }

    #[test]
    fn test_map_key() {
        let path = ErrorPath::feature("my-feature")
            .property("my-map")
            .map_key("my-key");
        assert_eq!("features/my-feature.my-map['my-key']", &path.path);
        assert_eq!(&["\"my-map\"", "{", "\"my-key\""], path.literals.as_slice());
    }

    #[test]
    fn test_enum_map_key() {
        let path = ErrorPath::feature("my-feature")
            .property("my-map")
            .enum_map_key("MyEnum", "my-variant");
        assert_eq!("features/my-feature.my-map[MyEnum#my-variant]", &path.path);
        assert_eq!(
            &["\"my-map\"", "{", "\"my-variant\""],
            path.literals.as_slice()
        );
    }

    #[test]
    fn test_array_index() {
        let path = ErrorPath::feature("my-feature")
            .property("my-array")
            .array_index(1);
        assert_eq!("features/my-feature.my-array[1]", &path.path);
        assert_eq!(&["\"my-array\"", "[", ","], path.literals.as_slice());

        let path = ErrorPath::feature("my-feature")
            .property("my-array")
            .array_index(0);
        assert_eq!("features/my-feature.my-array[0]", &path.path);
        assert_eq!(&["\"my-array\"", "["], path.literals.as_slice());
    }

    #[test]
    fn test_object_value() {
        let path = ErrorPath::feature("my-feature")
            .property("my-object")
            .object_value("MyObject");
        assert_eq!("features/my-feature.my-object#MyObject", &path.path);
        assert_eq!(&["\"my-object\"", "{"], path.literals.as_slice());
    }

    #[test]
    fn test_final_error() {
        //  1. `features/messaging.messages['my-message']#MessageData.is-control` expects a boolean,
        let path = ErrorPath::feature("messaging")
            .property("messages")
            .map_key("my-message")
            .object_value("MessageData")
            .property("is-control")
            .final_error("1");
        assert_eq!(
            "features/messaging.messages['my-message']#MessageData.is-control",
            &path.path
        );
        assert_eq!(
            &[
                "\"messages\"",
                "{",
                "\"my-message\"",
                "{",
                "\"is-control\"",
                "1"
            ],
            path.literals.as_slice()
        );

        //  2. `features/homescreen.sections-enabled[HomeScreenSection#pocket]` expects a boolean
        let path = ErrorPath::feature("homescreen")
            .property("sections-enabled")
            .enum_map_key("HomeScreenSection", "pocket")
            .final_error("1");
        assert_eq!(
            "features/homescreen.sections-enabled[HomeScreenSection#pocket]",
            &path.path
        );

        assert_eq!(
            &["\"sections-enabled\"", "{", "\"pocket\"", "1"],
            path.literals.as_slice()
        );
    }
}
