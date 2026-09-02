/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

/// The tables are static and borrowed; the types the embedder sees own their
/// strings, because borrowed data cannot cross an FFI boundary.
struct ColorDef {
    name: &'static str,
    code: &'static str,
    code_nova: &'static str,
    l10n_id: &'static str,
}

struct IconDef {
    name: &'static str,
    l10n_id: &'static str,
}

/// `code` is the legacy value, `code_nova` the refreshed one; picking between
/// them depends on a setting the embedder owns, so both are exposed and the
/// caller decides.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ContainerColor {
    pub name: String,
    pub code: String,
    pub code_nova: String,
    pub l10n_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ContainerIcon {
    pub name: String,
    pub l10n_id: String,
}

const COLORS: &[ColorDef] = &[
    ColorDef {
        name: "gray",
        code: "#7c7c7d",
        code_nova: "#949297",
        l10n_id: "user-context-color-gray",
    },
    ColorDef {
        name: "yellow",
        code: "#ffcb00",
        code_nova: "#db820e",
        l10n_id: "user-context-color-yellow",
    },
    ColorDef {
        name: "orange",
        code: "#ff9f00",
        code_nova: "#f4682c",
        l10n_id: "user-context-color-orange",
    },
    ColorDef {
        name: "red",
        code: "#ff613d",
        code_nova: "#ed566e",
        l10n_id: "user-context-color-red",
    },
    ColorDef {
        name: "pink",
        code: "#ff4bda",
        code_nova: "#db54bf",
        l10n_id: "user-context-color-pink",
    },
    ColorDef {
        name: "purple",
        code: "#af51f5",
        code_nova: "#b864ee",
        l10n_id: "user-context-color-purple",
    },
    ColorDef {
        name: "violet",
        code: "#764edd",
        code_nova: "#9871ff",
        l10n_id: "user-context-color-violet",
    },
    ColorDef {
        name: "blue",
        code: "#37adff",
        code_nova: "#5a87fd",
        l10n_id: "user-context-color-blue",
    },
    ColorDef {
        name: "cyan",
        code: "#00c79a",
        code_nova: "#10a4ca",
        l10n_id: "user-context-color-cyan",
    },
    ColorDef {
        name: "green",
        code: "#51cd00",
        code_nova: "#11ae84",
        l10n_id: "user-context-color-green",
    },
];

/// Legacy color names, accepted at the WebExtension API boundary and rewritten
/// to their canonical replacement by the 5 -> 6 migration.
const ALIASES: &[(&str, &str)] = &[("turquoise", "cyan"), ("toolbar", "gray")];

const ICONS: &[IconDef] = &[
    IconDef {
        name: "fingerprint",
        l10n_id: "user-context-icon-fingerprint",
    },
    IconDef {
        name: "briefcase",
        l10n_id: "user-context-icon-briefcase",
    },
    IconDef {
        name: "dollar",
        l10n_id: "user-context-icon-dollar",
    },
    IconDef {
        name: "cart",
        l10n_id: "user-context-icon-cart",
    },
    IconDef {
        name: "vacation",
        l10n_id: "user-context-icon-vacation",
    },
    IconDef {
        name: "gift",
        l10n_id: "user-context-icon-gift",
    },
    IconDef {
        name: "food",
        l10n_id: "user-context-icon-food",
    },
    IconDef {
        name: "fruit",
        l10n_id: "user-context-icon-fruit",
    },
    IconDef {
        name: "pet",
        l10n_id: "user-context-icon-pet",
    },
    IconDef {
        name: "tree",
        l10n_id: "user-context-icon-tree",
    },
    IconDef {
        name: "chill",
        l10n_id: "user-context-icon-chill",
    },
    IconDef {
        name: "circle",
        l10n_id: "user-context-icon-circle",
    },
    IconDef {
        name: "fence",
        l10n_id: "user-context-icon-fence",
    },
];

#[uniffi::export]
pub fn container_colors() -> Vec<ContainerColor> {
    COLORS
        .iter()
        .map(|color| ContainerColor {
            name: color.name.to_string(),
            code: color.code.to_string(),
            code_nova: color.code_nova.to_string(),
            l10n_id: color.l10n_id.to_string(),
        })
        .collect()
}

#[uniffi::export]
pub fn container_icons() -> Vec<ContainerIcon> {
    ICONS
        .iter()
        .map(|icon| ContainerIcon {
            name: icon.name.to_string(),
            l10n_id: icon.l10n_id.to_string(),
        })
        .collect()
}

#[uniffi::export]
pub fn container_color_aliases() -> HashMap<String, String> {
    ALIASES
        .iter()
        .map(|(legacy, canonical)| (legacy.to_string(), canonical.to_string()))
        .collect()
}

#[uniffi::export]
pub fn resolve_color(name: &str) -> String {
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, canonical)| (*canonical).to_string())
        .unwrap_or_else(|| name.to_string())
}

fn find_color(name: &str) -> Option<&'static ColorDef> {
    COLORS.iter().find(|color| color.name == name)
}

fn find_icon(name: &str) -> Option<&'static IconDef> {
    ICONS.iter().find(|icon| icon.name == name)
}

#[uniffi::export]
pub fn color_code(name: &str, nova: bool) -> Option<String> {
    find_color(name).map(|color| {
        if nova {
            color.code_nova.to_string()
        } else {
            color.code.to_string()
        }
    })
}

#[uniffi::export]
pub fn color_l10n_id(name: &str) -> Option<String> {
    find_color(name).map(|color| color.l10n_id.to_string())
}

#[uniffi::export]
pub fn icon_l10n_id(name: &str) -> Option<String> {
    find_icon(name).map(|icon| icon.l10n_id.to_string())
}

pub(crate) fn canonical_color(name: &str) -> Option<String> {
    let resolved = resolve_color(name);
    find_color(&resolved).map(|_| resolved)
}

#[uniffi::export]
pub fn is_known_icon(name: &str) -> bool {
    find_icon(name).is_some()
}
