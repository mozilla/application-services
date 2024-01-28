/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

const NAME_PREFIXES: &[&str] = &[
    "1lt", "1st", "2lt", "2nd", "3rd", "admiral", "capt", "captain", "col", "cpt", "dr", "gen",
    "general", "lcdr", "lt", "ltc", "ltg", "ltjg", "maj", "major", "mg", "mr", "mrs", "ms",
    "pastor", "prof", "rep", "reverend", "rev", "sen",
];

const NAME_SUFFIXES: &[&str] = &[
    "b.a", "ba", "d.d.s", "dds", "i", "ii", "iii", "iv", "ix", "jr", "m.a", "m.d", "ma", "md",
    "ms", "ph.d", "phd", "sr", "v", "vi", "vii", "viii", "x",
];

const FAMILY_NAME_PREFIXES: &[&str] = &[
    "d'", "de", "del", "der", "di", "la", "le", "mc", "san", "st", "ter", "van", "von",
];

// The common and non-ambiguous CJK surnames (last names) that have more than
// one character.
const COMMON_CJK_MULTI_CHAR_SURNAMES: &[&str] = &[
    // Korean, taken from the list of surnames:
    // https://ko.wikipedia.org/wiki/%ED%95%9C%EA%B5%AD%EC%9D%98_%EC%84%B1%EC%94%A8_%EB%AA%A9%EB%A1%9D
    "남궁", "사공", "서문", "선우", "제갈", "황보", "독고", "망절",
    // Chinese, taken from the top 10 Chinese 2-character surnames:
    // https://zh.wikipedia.org/wiki/%E8%A4%87%E5%A7%93#.E5.B8.B8.E8.A6.8B.E7.9A.84.E8.A4.87.E5.A7.93
    // Simplified Chinese (mostly mainland China)
    "欧阳", "令狐", "皇甫", "上官", "司徒", "诸葛", "司马", "宇文", "呼延", "端木",
    // Traditional Chinese (mostly Taiwan)
    "張簡", "歐陽", "諸葛", "申屠", "尉遲", "司馬", "軒轅", "夏侯",
];

// All Korean surnames that have more than one character, even the
// rare/ambiguous ones.
const KOREAN_MULTI_CHAR_SURNAMES: &[&str] = &[
    "강전", "남궁", "독고", "동방", "망절", "사공", "서문", "선우", "소봉", "어금", "장곡", "제갈",
    "황목", "황보",
];

// The middle dot is used as a separator for foreign names in Japanese.
const MIDDLE_DOT: &[char] = &[
    '\u{30FB}', // KATAKANA MIDDLE DOT
    '\u{00B7}', // A (common?) typo for "KATAKANA MIDDLE DOT}"
];

const CJK_RANGE: &[(char, char)] = &[
    ('\u{1100}', '\u{11FF}'), // Hangul Jamo
    ('\u{3040}', '\u{309F}'), // Hiragana
    ('\u{30A0}', '\u{30FF}'), // Katakana
    ('\u{3105}', '\u{312C}'), // Bopomofo
    ('\u{3130}', '\u{318F}'), // Hangul Compatibility Jamo
    ('\u{31F0}', '\u{31FF}'), // Katakana Phonetic Extensions
    ('\u{3200}', '\u{32FF}'), // Enclosed CJK Letters and Months
    ('\u{3400}', '\u{4DBF}'), // CJK unified ideographs Extension A
    ('\u{4E00}', '\u{9FFF}'), // CJK Unified Ideographs
    ('\u{A960}', '\u{A97F}'), // Hangul Jamo Extended-A
    ('\u{AC00}', '\u{D7AF}'), // Hangul Syllables
    ('\u{D7B0}', '\u{D7FF}'), // Hangul Jamo Extended-B
    ('\u{FF00}', '\u{FFEF}'), // Halfwidth and Fullwidth Forms
];

const HANGUL_RANGE: &[(char, char)] = &[
    ('\u{1100}', '\u{11FF}'), // Hangul Jamo
    ('\u{3130}', '\u{318F}'), // Hangul Compatibility Jamo
    ('\u{A960}', '\u{A97F}'), // Hangul Jamo Extended-A
    ('\u{AC00}', '\u{D7AF}'), // Hangul Syllables
    ('\u{D7B0}', '\u{D7FF}'), // Hangul Jamo Extended-B
];

#[derive(PartialEq, Debug, Default)]
pub(crate) struct NameParts {
    pub(crate) given: String,
    pub(crate) middle: String,
    pub(crate) family: String,
}

fn is_name_separator(c: char) -> bool {
    c.is_whitespace() || MIDDLE_DOT.contains(&c) || c == ','
}

fn contains_string(set: &[&str], token: &str) -> bool {
    let target = token.trim_end_matches('.').to_lowercase();
    set.contains(&target.as_str())
}

fn strip_prefixes<'a>(name_tokens: &'a [&'a str]) -> &'a [&'a str] {
    name_tokens
        .iter()
        .position(|token| !contains_string(NAME_PREFIXES, token))
        .map_or(&[], |index| &name_tokens[index..])
}

fn strip_suffixes<'a>(name_tokens: &'a [&'a str]) -> &'a [&'a str] {
    name_tokens
        .iter()
        .rposition(|token| !contains_string(NAME_SUFFIXES, token))
        .map_or(&[], |index| &name_tokens[..=index])
}

fn is_char_in_range(c: char, range: &[(char, char)]) -> bool {
    range.iter().any(|&(start, end)| c >= start && c <= end)
}

fn is_cjk_name(name: &str) -> bool {
    // TODO(issam): Do we need these checks inside each function?
    if name.is_empty() || name.split_whitespace().count() > 2 {
        return false;
    }

    name.split_whitespace().all(|part| {
        part.chars()
            .all(|c| MIDDLE_DOT.contains(&c) || is_char_in_range(c, CJK_RANGE))
    })
}

fn is_korean_name(name: &str) -> bool {
    // TODO(issam): Do we need these checks inside each function?
    if name.is_empty() {
        return false;
    }

    name.split_whitespace()
        .all(|part| part.chars().all(|c| is_char_in_range(c, HANGUL_RANGE)))
}

fn get_cjk_surname_length(name: &str) -> usize {
    let surnames = if is_korean_name(name) && name.chars().count() > 3 {
        KOREAN_MULTI_CHAR_SURNAMES
    } else {
        COMMON_CJK_MULTI_CHAR_SURNAMES
    };

    surnames
        .iter()
        .any(|&surname| name.starts_with(surname))
        .then(|| 2)
        .unwrap_or(1)
}

fn split_cjk_name(name_tokens: &[&str]) -> Option<NameParts> {
    match name_tokens.len() {
        1 => {
            let name = name_tokens[0];
            let surname_length = get_cjk_surname_length(name);
            Some(NameParts {
                given: name.chars().skip(surname_length).collect(),
                family: name.chars().take(surname_length).collect(),
                ..Default::default()
            })
        }
        2 => Some(NameParts {
            given: name_tokens[1].to_string(),
            family: name_tokens[0].to_string(),
            ..Default::default()
        }),
        _ => None,
    }
}

pub(crate) fn join_name_parts(name_parts: &NameParts) -> String {
    if is_cjk_name(&name_parts.given)
        && is_cjk_name(&name_parts.family)
        && name_parts.middle.is_empty()
    {
        return format!("{}{}", name_parts.family, name_parts.given);
    }

    [
        name_parts.given.as_str(),
        name_parts.middle.as_str(),
        name_parts.family.as_str(),
    ]
    .iter()
    .filter(|&part| !part.is_empty())
    .cloned()
    .collect::<Vec<&str>>()
    .join(" ")
}

pub(crate) fn split_name(name: &str) -> NameParts {
    if name.is_empty() {
        return NameParts::default();
    }

    let name_tokens: Vec<&str> = name
        .trim()
        .split(is_name_separator)
        .filter(|s| !s.is_empty())
        .collect();

    // TODO(issam): Refactor. Still not too happy about this block below.
    let stripped_prefixes = strip_prefixes(&name_tokens).to_vec();

    if is_cjk_name(name) {
        if let Some(cjk_parts) = split_cjk_name(&stripped_prefixes) {
            return cjk_parts;
        }
    }

    let mut stripped_suffixes: Vec<&str> = stripped_prefixes.to_vec();

    if name_tokens.len() > 2 {
        stripped_suffixes = strip_suffixes(&stripped_prefixes).to_vec();
    }

    match stripped_suffixes.len() {
        0 => NameParts {
            given: name.to_string(),
            ..Default::default()
        },
        1 => NameParts {
            given: stripped_suffixes[0].to_string(),
            ..Default::default()
        },
        _ => {
            let mut family_tokens = vec![stripped_suffixes.pop().unwrap()];
            while !stripped_suffixes.is_empty()
                && contains_string(FAMILY_NAME_PREFIXES, stripped_suffixes.last().unwrap())
            {
                family_tokens.insert(0, stripped_suffixes.pop().unwrap());
            }

            let family = family_tokens.join(" ");
            let middle = if stripped_suffixes.len() >= 2 {
                stripped_suffixes.pop().unwrap().to_string()
            } else {
                String::new()
            };
            let given = stripped_suffixes.join(" ");

            NameParts {
                given,
                middle,
                family,
            }
        }
    }
}

// These tests were ported from:
// https://searchfox.org/mozilla-central/rev/2a867dd1ab015c3ef24b774a57709fb3b3dc4961/toolkit/components/formautofill/shared/FormAutofillNameUtils.sys.mjs
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_name_split_and_join {
        ($desc:expr, full_name: $full_name:expr, expected_full_name: $expected_full_name:expr, name_parts: { given: $given:expr, middle: $middle:expr, family: $family:expr }) => {{
            println!("Starting testcase: {}", $desc);

            // Creating a NameParts instance from provided parts
            let expected_name_parts = NameParts {
                given: $given.to_string(),
                middle: $middle.to_string(),
                family: $family.to_string(),
            };

            // Test split_name function
            let name_parts = split_name($full_name);
            assert_eq!(
                name_parts, expected_name_parts,
                "Test failed in split_name for: {}",
                $desc
            );

            // Test join_name_parts function
            let full_name_result = join_name_parts(&expected_name_parts);
            assert_eq!(
                full_name_result, $expected_full_name,
                "Test failed in join_name_parts for: {}",
                $desc
            );
        }};
    }

    #[test]
    fn test_name_functions() {
        test_name_split_and_join!(
             "Empty name",
            full_name: "",
            expected_full_name: "",
            name_parts:  {
                given: "",
                middle: "",
                family: ""
            }
        );

        test_name_split_and_join!(
             "Full name including prefixes and suffixes and comma",
            full_name: "Mr. John Doe, Jr.",
            expected_full_name: "John Doe",
            name_parts:  {
                given: "John",
                middle: "",
                family: "Doe"
            }
        );

        test_name_split_and_join!(
             "Full name including given, middle and family names",
            full_name: "Homer Jay Simpson",
            expected_full_name: "Homer Jay Simpson",
            name_parts:  {
                given: "Homer",
                middle: "Jay",
                family: "Simpson"
            }
        );

        test_name_split_and_join!(
             "No middle name",
            full_name: "Moe Szyslak",
            expected_full_name: "Moe Szyslak",
            name_parts:  {
                given: "Moe",
                middle: "",
                family: "Szyslak"
            }
        );

        test_name_split_and_join!(
             "Common name prefixes removed",
            full_name: "Reverend Timothy Lovejoy",
            expected_full_name: "Timothy Lovejoy",
            name_parts:  {
                given: "Timothy",
                middle: "",
                family: "Lovejoy"
            }
        );

        test_name_split_and_join!(
             "Common name suffixes removed",
            full_name: "John Frink Phd",
            expected_full_name: "John Frink",
            name_parts:  {
                given: "John",
                middle: "",
                family: "Frink"
            }
        );

        test_name_split_and_join!(
             "Exception to the name suffix removal",
            full_name: "John Ma",
            expected_full_name: "John Ma",
            name_parts:  {
                given: "John",
                middle: "",
                family: "Ma"
            }
        );

        test_name_split_and_join!(
            "Common family name prefixes not considered a middle name",
           full_name: "Milhouse Van Houten",
           expected_full_name: "Milhouse Van Houten",

           name_parts:  {
               given: "Milhouse",
               middle: "",
               family: "Van Houten"
           }
        );

        // CJK names have reverse order (surname goes first, given name goes second).
        test_name_split_and_join!(
             "Chinese name, Unihan",
            full_name: "孫 德明",
            expected_full_name: "孫德明",
            name_parts:  {
                given: "德明",
                middle: "",
                family: "孫"
            }
        );

        test_name_split_and_join!(
             "Chinese name, Unihan, IDEOGRAPHIC SPACE",
            full_name: "孫　德明",
            expected_full_name: "孫德明",
            name_parts:  {
                given: "德明",
                middle: "",
                family: "孫"
            }
        );

        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "홍 길동",
            expected_full_name: "홍길동",
            name_parts:  {
                given: "길동",
                middle: "",
                family: "홍"
            }
        );
        test_name_split_and_join!(
            "Japanese name, Unihan",
           full_name: "山田 貴洋",
           expected_full_name: "山田貴洋",
           name_parts:  {
               given: "貴洋",
               middle: "",
               family: "山田"
           }
        );

        // In Japanese, foreign names use 'KATAKANA MIDDLE DOT' (U+30FB) as a
        // separator. There is no consensus for the ordering. For now, we use the same
        // ordering as regular Japanese names ("last・first").
        test_name_split_and_join!(
            "Foreign name in Japanese, Katakana",
           full_name: "ゲイツ・ビル",
           expected_full_name: "ゲイツビル",
           name_parts:  {
               given: "ビル",
               middle: "",
               family: "ゲイツ"
           }
        );

        // 'KATAKANA MIDDLE DOT' is occasionally typoed as 'MIDDLE DOT' (U+00B7).
        test_name_split_and_join!(
            "Foreign name in Japanese, Katakana",
           full_name: "ゲイツ·ビル",
           expected_full_name: "ゲイツビル",
           name_parts:  {
               given: "ビル",
               middle: "",
               family: "ゲイツ"
           }
        );

        // CJK names don't usually have a space in the middle, but most of the time,
        // the surname is only one character (in Chinese & Korean).
        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "최성훈",
            expected_full_name: "최성훈",
            name_parts:  {
                given: "성훈",
                middle: "",
                family: "최"
            }
        );

        test_name_split_and_join!(
             "(Simplified) Chinese name, Unihan",
            full_name: "刘翔",
            expected_full_name: "刘翔",

            name_parts:  {
                given: "翔",
                middle: "",
                family: "刘"
            }
        );

        test_name_split_and_join!(
            "(Traditional) Chinese name, Unihan",
           full_name: "劉翔",
           expected_full_name: "劉翔",

           name_parts:  {
               given: "翔",
               middle: "",
               family: "劉"
           }
        );

        // There are a few exceptions. Occasionally, the surname has two characters.
        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "남궁도",
            expected_full_name: "남궁도",

            name_parts:  {
                given: "도",
                middle: "",
                family: "남궁"
            }
        );

        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "황보혜정",
            expected_full_name: "황보혜정",

            name_parts:  {
                given: "혜정",
                middle: "",
                family: "황보"
            }
        );

        test_name_split_and_join!(
            "(Traditional) Chinese name, Unihan",
           full_name: "歐陽靖",
           expected_full_name: "歐陽靖",

           name_parts:  {
               given: "靖",
               middle: "",
               family: "歐陽"
           }
        );

        // In Korean, some 2-character surnames are rare/ambiguous, like "강전": "강"
        // is a common surname, and "전" can be part of a given name. In those cases,
        // we assume it's 1/2 for 3-character names, or 2/2 for 4-character names.
        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "강전희",
            expected_full_name: "강전희",

            name_parts:  {
                given: "전희",
                middle: "",
                family: "강"
            }
        );

        test_name_split_and_join!(
            "Korean name, Hangul",
           full_name: "황목치승",
           expected_full_name: "황목치승",

           name_parts:  {
               given: "치승",
               middle: "",
               family: "황목"
           }
        );

        // It occasionally happens that a full name is 2 characters, 1/1.
        test_name_split_and_join!(
             "Korean name, Hangul",
            full_name: "이도",
            expected_full_name: "이도",
            name_parts:  {
                given: "도",
                middle: "",
                family: "이"
            }
        );

        test_name_split_and_join!(
            "Korean name, Hangul",
           full_name: "孫文",
           expected_full_name: "孫文",
           name_parts:  {
               given: "文",
               middle: "",
               family: "孫"
           }
        );

        // These are no CJK names for us, they're just bogus.
        test_name_split_and_join!(
             "Bogus",
            full_name: "Homer シンプソン",
            expected_full_name: "Homer シンプソン",
            name_parts:  {
                given: "Homer",
                middle: "",
                family: "シンプソン"
            }
        );

        test_name_split_and_join!(
             "Bogus",
            full_name: "ホーマー Simpson",
            expected_full_name: "ホーマー Simpson",
            name_parts:  {
                given: "ホーマー",
                middle: "",
                family: "Simpson"
            }
        );

        test_name_split_and_join!(
            "CJK has a middle-name, too unusual",
           full_name: "반 기 문",
           expected_full_name: "반 기 문",
           name_parts:  {
               given: "반",
               middle: "기",
               family: "문"
           }
        );
    }
}
