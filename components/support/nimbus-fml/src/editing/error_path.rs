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
    literals: Vec<String>,
    pub(crate) path: String,
}

/// Chained Constructors
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

/// Accessors
#[allow(dead_code)]
#[cfg(feature = "client-lib")]
impl ErrorPath {
    pub(crate) fn line_col(&self, src: &str) -> (usize, usize) {
        line_col(src, self.literals.iter().map(|s| s.as_str()))
    }
}

impl ErrorPath {
    pub(crate) fn last_token(&self) -> Option<&str> {
        self.literals.last().map(|x| x.as_str())
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

#[allow(dead_code)]
fn line_col<'a>(src: &'a str, path: impl Iterator<Item = &'a str>) -> (usize, usize) {
    let mut lines = src.lines();

    let mut line_no = 0;
    let mut col_no = 0;

    let mut first_match = false;
    let mut line = lines.next().unwrap_or_default();

    for p in path {
        loop {
            // If we haven't had our first match of the line, then start there at the beginning.
            // Otherwise, start one char on from where we were last time.
            let start = if !first_match { 0 } else { col_no + 1 };

            // if let Some(i) = cur[start..].find(&p).map(|i| i + start) {
            if let Some(i) = find_index(line, p, start) {
                col_no = i;
                first_match = true;
                break;
            } else if let Some(next) = lines.next() {
                // we try the next line!
                line = next;
                line_no += 1;
                first_match = false;
                col_no = 0;
            } else {
                // we've run out of lines, so we should return
                return (0, 0);
            }
        }
    }

    (line_no, col_no)
}

/// Find the index in `line` of the next instance of `pattern`, after `start`
///
/// A current weakness with this method is that it is not unicode aware.
#[allow(dead_code)]
fn find_index(line: &str, pattern: &str, start: usize) -> Option<usize> {
    line.match_indices(pattern)
        .find(|(i, _)| i >= &start)
        .map(|(i, _)| i)
}

#[cfg(test)]
mod construction_tests {
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

#[cfg(test)]
mod line_col_tests {

    use super::*;
    use crate::error::Result;

    #[test]
    fn test_find_err() -> Result<()> {
        fn do_test(s: &str, path: &[&str], expected: (usize, usize)) {
            let p = path.last().unwrap();
            let path = path.iter().cloned();
            assert_eq!(
                line_col(s, path),
                expected,
                "Can't find \"{p}\" at {expected:?} in {s}"
            );
        }

        fn do_multi(s: &[&str], path: &[&str], expected: (usize, usize)) {
            let s = s.join("\n");
            do_test(&s, path, expected);
        }

        do_test("ab cd", &["ab", "cd"], (0, 3));

        do_test("ab ab", &["ab"], (0, 0));
        do_test("ab ab", &["ab", "ab"], (0, 3));

        do_multi(
            &["ab xx cd", "xx ef xx gh", "ij xx"],
            &["ab", "cd", "gh", "xx"],
            (2, 3),
        );

        do_multi(
            &[
                "{",                       // 0
                "  boolean: true,",        // 1
                "  object: {",             // 2
                "    integer: \"string\"", // 3
                "  }",                     // 4
                "}",                       // 5
            ],
            &["object", "integer", "\"string\""],
            (3, 13),
        );

        // pathological case
        do_multi(
            &[
                "{",                       // 0
                "  boolean: true,",        // 1
                "  object: {",             // 2
                "    integer: 1,",         // 3
                "    astring: \"string\"", // 4
                "  },",                    // 5
                "  integer: \"string\"",   // 6
                "}",                       // 7
            ],
            &["integer", "\"string\""],
            (4, 13),
        );

        Ok(())
    }

    #[test]
    fn test_find_index_from() -> Result<()> {
        assert_eq!(find_index("012345601", "01", 0), Some(0));
        assert_eq!(find_index("012345601", "01", 1), Some(7));
        assert_eq!(find_index("012345602", "01", 1), None);

        // TODO unicode indexing does not work.
        // assert_eq!(find_index("åéîø token", "token", 0), Some(5));
        Ok(())
    }
}
