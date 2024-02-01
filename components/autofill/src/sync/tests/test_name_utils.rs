use crate::sync::address::name_utils::{is_cjk_name, join_name_parts, split_name, NameParts};
use serde_json::{json, Value};

// These tests were ported from desktop:
// - test_isCJKName.js (https://searchfox.org/mozilla-central/rev/c036a209f5a2c3163d1277ee2b7becaab2f79dbd/browser/extensions/formautofill/test/unit/test_isCJKName.js)
// - test_nameUtils.js (https://searchfox.org/mozilla-central/rev/896042a1a71066254ceb5291f016ca3dbca21cb7/browser/extensions/formautofill/test/unit/test_nameUtils.js)
lazy_static::lazy_static! {
    static ref CJK_NAME_TESTCASES : Value =  json!([
        {
            // Non-CJK language with only ASCII characters.
            "name": "Homer Jay Simpson",
            "expected_result": false,
        },
        {
            // Non-CJK language with some ASCII characters.
            "name": "Éloïse Paré",
            "expected_result": false,
        },
    ]);

    static ref NAME_TESTCASES : Value =  json!([
        {
            "description": "Full name including given, middle and family names",
            "full_name": "Homer Jay Simpson",
            "name_parts": {
            "given": "Homer",
            "middle": "Jay",
            "family": "Simpson",
            },
        },
        {
            "description": "No middle name",
            "full_name": "Moe Szyslak",
            "name_parts": {
            "given": "Moe",
            "middle": "",
            "family": "Szyslak",
            },
        },
        {
            "description": "Common name prefixes removed",
            "full_name": "Reverend Timothy Lovejoy",
            "name_parts": {
            "given": "Timothy",
            "middle": "",
            "family": "Lovejoy",
            },
            "expected_full_name": "Timothy Lovejoy",
        },
        {
            "description": "Common name suffixes removed",
            "full_name": "John Frink Phd",
            "name_parts": {
            "given": "John",
            "middle": "",
            "family": "Frink",
            },
            "expected_full_name": "John Frink",
        },
        {
            "description": "Exception to the name suffix removal",
            "full_name": "John Ma",
            "name_parts": {
            "given": "John",
            "middle": "",
            "family": "Ma",
            },
        },
        {
            "description": "Common family name prefixes not considered a middle name",
            "full_name": "Milhouse Van Houten",
            "name_parts": {
            "given": "Milhouse",
            "middle": "",
            "family": "Van Houten",
            },
        },

        // CJK names have reverse order (surname goes first, given name goes second).
        {
            "description": "Chinese name, Unihan",
            "full_name": "孫 德明",
            "name_parts": {
            "given": "德明",
            "middle": "",
            "family": "孫",
            },
            "expected_full_name": "孫德明",
        },
        {
            "description": "Chinese name, Unihan, 'IDEOGRAPHIC SPACE'",
            "full_name": "孫　德明",
            "name_parts": {
            "given": "德明",
            "middle": "",
            "family": "孫",
            },
            "expected_full_name": "孫德明",
        },
        {
            "description": "Korean name, Hangul",
            "full_name": "홍 길동",
            "name_parts": {
            "given": "길동",
            "middle": "",
            "family": "홍",
            },
            "expected_full_name": "홍길동",
        },
        {
            "description": "Japanese name, Unihan",
            "full_name": "山田 貴洋",
            "name_parts": {
            "given": "貴洋",
            "middle": "",
            "family": "山田",
            },
            "expected_full_name": "山田貴洋",
        },

        // In Japanese, foreign names use 'KATAKANA MIDDLE DOT' (U+30FB) as a
        // separator. There is no consensus for the ordering. For now, we use the same
        // ordering as regular Japanese names ("last・first").
        {
            "description": "Foreign name in Japanese, Katakana",
            "full_name": "ゲイツ・ビル",
            "name_parts": {
            "given": "ビル",
            "middle": "",
            "family": "ゲイツ",
            },
            "expected_full_name": "ゲイツビル",
        },

        // 'KATAKANA MIDDLE DOT' is occasionally typoed as 'MIDDLE DOT' (U+00B7).
        {
            "description": "Foreign name in Japanese, Katakana",
            "full_name": "ゲイツ·ビル",
            "name_parts": {
            "given": "ビル",
            "middle": "",
            "family": "ゲイツ",
            },
            "expected_full_name": "ゲイツビル",
        },

        // CJK names don't usually have a space in the middle, but most of the time,
        // the surname is only one character (in Chinese & Korean).
        {
            "description": "Korean name, Hangul",
            "full_name": "최성훈",
            "name_parts": {
            "given": "성훈",
            "middle": "",
            "family": "최",
            },
        },
        {
            "description": "(Simplified) Chinese name, Unihan",
            "full_name": "刘翔",
            "name_parts": {
            "given": "翔",
            "middle": "",
            "family": "刘",
            },
        },
        {
            "description": "(Traditional) Chinese name, Unihan",
            "full_name": "劉翔",
            "name_parts": {
            "given": "翔",
            "middle": "",
            "family": "劉",
            },
        },

        // There are a few exceptions. Occasionally, the surname has two characters.
        {
            "description": "Korean name, Hangul",
            "full_name": "남궁도",
            "name_parts": {
            "given": "도",
            "middle": "",
            "family": "남궁",
            },
        },
        {
            "description": "Korean name, Hangul",
            "full_name": "황보혜정",
            "name_parts": {
            "given": "혜정",
            "middle": "",
            "family": "황보",
            },
        },
        {
            "description": "(Traditional) Chinese name, Unihan",
            "full_name": "歐陽靖",
            "name_parts": {
            "given": "靖",
            "middle": "",
            "family": "歐陽",
            },
        },

        // In Korean, some 2-character surnames are rare/ambiguous, like "강전": "강"
        // is a common surname, and "전" can be part of a given name. In those cases,
        // we assume it's 1/2 for 3-character names, or 2/2 for 4-character names.
        {
            "description": "Korean name, Hangul",
            "full_name": "강전희",
            "name_parts": {
            "given": "전희",
            "middle": "",
            "family": "강",
            },
        },
        {
            "description": "Korean name, Hangul",
            "full_name": "황목치승",
            "name_parts": {
            "given": "치승",
            "middle": "",
            "family": "황목",
            },
        },

        // It occasionally happens that a full name is 2 characters, 1/1.
        {
            "description": "Korean name, Hangul",
            "full_name": "이도",
            "name_parts": {
            "given": "도",
            "middle": "",
            "family": "이",
            },
        },
        {
            "description": "Korean name, Hangul",
            "full_name": "孫文",
            "name_parts": {
            "given": "文",
            "middle": "",
            "family": "孫",
            },
        },

        // These are no CJK names for us, they're just bogus.
        {
            "description": "Bogus",
            "full_name": "Homer シンプソン",
            "name_parts": {
            "given": "Homer",
            "middle": "",
            "family": "シンプソン",
            },
        },
        {
            "description": "Bogus",
            "full_name": "ホーマー Simpson",
            "name_parts": {
            "given": "ホーマー",
            "middle": "",
            "family": "Simpson",
            },
        },
        {
            "description": "CJK has a middle-name, too unusual",
            "full_name": "반 기 문",
            "name_parts": {
            "given": "반",
            "middle": "기",
            "family": "문",
            },
        }
    ]);
}

#[test]
fn test_is_cjk_name() {
    let test_cases = CJK_NAME_TESTCASES
        .as_array()
        .expect("CJK_NAME_TESTCASES is not an array");

    for test_case in test_cases {
        let name = test_case["name"]
            .as_str()
            .expect("Name not found or not a string");
        let expected_result = test_case["expected_result"]
            .as_bool()
            .expect("Expected result not found or not a boolean");

        assert_eq!(is_cjk_name(name), expected_result);
    }
}

fn name_parts_from_json(json: &serde_json::Value) -> NameParts {
    let name_parts_obj = json["name_parts"]
        .as_object()
        .expect("name_parts is not an object");
    NameParts {
        given: name_parts_obj
            .get("given")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        middle: name_parts_obj
            .get("middle")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        family: name_parts_obj
            .get("family")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    }
}

#[test]
fn test_split_name() {
    let test_cases = NAME_TESTCASES
        .as_array()
        .expect("NAME_TESTCASES is not an array");

    for test_case in test_cases {
        let full_name = test_case["full_name"]
            .as_str()
            .expect("full_name not found or not a string");

        let name_parts = name_parts_from_json(test_case);

        assert_eq!(split_name(full_name), name_parts);
    }
}

#[test]
fn test_join_name_parts() {
    let test_cases = NAME_TESTCASES
        .as_array()
        .expect("NAME_TESTCASES is not an array");

    for test_case in test_cases {
        let full_name = test_case["full_name"]
            .as_str()
            .expect("full_name not found or not a string");
        let expected_full_name = test_case["expected_full_name"]
            .as_str()
            .unwrap_or(full_name);

        let name_parts = name_parts_from_json(test_case);

        assert_eq!(join_name_parts(&name_parts), expected_full_name);
    }
}
