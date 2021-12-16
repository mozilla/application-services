/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
#if canImport(MozillaRustComponents)
    import MozillaRustComponents
#endif

/// Snarfed from firefox-ios, although we don't have the fake desktop root,
/// and we only have the `All` Set.
public enum BookmarkRoots {
    public static let RootGUID = "root________"
    public static let MobileFolderGUID = "mobile______"
    public static let MenuFolderGUID = "menu________"
    public static let ToolbarFolderGUID = "toolbar_____"
    public static let UnfiledFolderGUID = "unfiled_____"

    public static let All = Set<String>([
        BookmarkRoots.RootGUID,
        BookmarkRoots.MobileFolderGUID,
        BookmarkRoots.MenuFolderGUID,
        BookmarkRoots.ToolbarFolderGUID,
        BookmarkRoots.UnfiledFolderGUID,
    ])
}
