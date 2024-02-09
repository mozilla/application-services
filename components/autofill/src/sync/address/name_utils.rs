/* This Source Code Form is subject to the terms of the Mozilla Public
* License, v. 2.0. If a copy of the MPL was not distributed with this
* file, You can obtain one at http://mozilla.org/MPL/2.0/.
*/

// This code is a port of the FormAutofillNameUtils.sys.js file from central:
// https://searchfox.org/mozilla-central/rev/2a867dd1ab015c3ef24b774a57709fb3b3dc4961/toolkit/components/formautofill/shared/FormAutofillNameUtils.sys.mjs
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

pub(crate) fn is_cjk_name(name: &str) -> bool {
    if name.is_empty() || name.split_whitespace().count() > 2 {
        return false;
    }

    name.split_whitespace().all(|part| {
        part.chars()
            .all(|c| MIDDLE_DOT.contains(&c) || is_char_in_range(c, CJK_RANGE))
    })
}

fn is_korean_name(name: &str) -> bool {
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

    if surnames.iter().any(|&surname| name.starts_with(surname)) {
        2
    } else {
        1
    }
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

fn handle_multiple_suffixes(suffixes: &[&str]) -> NameParts {
    let mut suffixes = suffixes.to_vec(); // Convert to Vec<&str> if mutation is needed

    let family_tokens = extract_family_tokens(&mut suffixes);
    let family = family_tokens.join(" ");

    let middle = if suffixes.len() >= 2 {
        suffixes.pop().unwrap().to_string()
    } else {
        String::new()
    };

    let given = suffixes.join(" ");

    NameParts {
        given,
        middle,
        family,
    }
}

fn extract_family_tokens(suffixes: &mut Vec<&str>) -> Vec<String> {
    let mut family_tokens = vec![suffixes.pop().unwrap().to_string()];
    while !suffixes.is_empty() && contains_string(FAMILY_NAME_PREFIXES, suffixes.last().unwrap()) {
        family_tokens.insert(0, suffixes.pop().unwrap().to_string());
    }
    family_tokens
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

    let stripped_prefixes = strip_prefixes(&name_tokens);

    if is_cjk_name(name) {
        if let Some(cjk_parts) = split_cjk_name(stripped_prefixes) {
            return cjk_parts;
        }
    }

    let stripped_suffixes = if name_tokens.len() > 2 {
        strip_suffixes(stripped_prefixes)
    } else {
        stripped_prefixes
    };

    match stripped_suffixes {
        [] => NameParts {
            given: name.to_string(),
            ..Default::default()
        },
        [given] => NameParts {
            given: given.to_string(),
            ..Default::default()
        },
        _ => handle_multiple_suffixes(stripped_suffixes),
    }
}
