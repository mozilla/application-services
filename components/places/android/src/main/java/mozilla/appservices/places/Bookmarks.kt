/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import mozilla.appservices.places.uniffi.BookmarkItem

/**
 * Enumeration of the ids of the roots of the bookmarks tree.
 *
 * There are 5 "roots" in the bookmark tree. The actual root
 * (which has no parent), and it's 4 children (which have the
 * actual root as their parent).
 *
 * You cannot delete or move any of these items.
 */
enum class BookmarkRoot(val id: String) {
    Root("root________"),
    Menu("menu________"),
    Toolbar("toolbar_____"),
    Unfiled("unfiled_____"),
    Mobile("mobile______"),
}

/**
 * The methods provided by a read-only or a read-write bookmarks connection.
 */
interface ReadableBookmarksConnection : InterruptibleConnection {
    /**
     * Returns the bookmark subtree rooted at `rootGUID`. This differs from
     * `getBookmark` in that it populates folder children.
     *
     * Specifically, any [BookmarkFolder]s in the returned value will have their
     * `children` list populated, and not just `childGUIDs` (Note: if
     * `recursive = false` is passed, then this is only performed for direct
     * children, and not for grandchildren).
     *
     * @param rootGUID the GUID where to start the tree.
     *
     * @param recursive Whether or not to return more than a single level of children for folders.
     * If false, then any folders which are children of the requested node will *only* have their
     * `childGUIDs` populated, and *not* their `children`.
     *
     * @return The bookmarks tree starting at `rootGUID`, or null if the provided
     * id didn't refer to a known bookmark item.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun getBookmarksTree(rootGUID: Guid, recursive: Boolean): BookmarkItem?

    /**
     * Returns the information about the bookmark with the provided id. This differs from
     * `getBookmarksTree` in that it does not populate the `children` list if `guid` refers
     * to a folder (However, its `childGUIDs` list will be populated).
     *
     * @param guid the guid of the bookmark to fetch.
     * @return The bookmark node, or null if the provided
     *         guid didn't refer to a known bookmark item.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun getBookmark(guid: Guid): BookmarkItem?

    /**
     * Returns the list of bookmarks with the provided URL.
     *
     * Note that if the URL is not percent-encoded/punycoded, that will be performed
     * internally, and so the returned bookmarks may not have an identical URL to
     * the one passed in (however, it will be the same according to the
     * [URL standard](https://url.spec.whatwg.org/)).
     *
     * @param url The url to search for.
     * @return A list of bookmarks that have the requested URL.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun getBookmarksWithURL(url: String): List<BookmarkItem>

    /**
     * Returns the URL for the provided search keyword, if one exists.
     *
     * @param The search keyword.
     * @return The bookmarked URL for the keyword, if set.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun getBookmarkUrlForKeyword(keyword: String): Url?

    /**
     * Returns the list of bookmarks that match the provided search string.
     *
     * The order of the results is unspecified.
     *
     * @param query The search query
     * @param limit The maximum number of items to return.
     * @return A list of bookmarks where either the URL or the title contain a word
     * (e.g. space separated item) from the query.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun searchBookmarks(query: String, limit: Int): List<BookmarkItem>

    /**
     * Returns the list of most recently added bookmarks.
     *
     * The result list be in order of time of addition, descending (more recent
     * additions first), and will contain no folder or separator nodes.
     *
     * @param limit The maximum number of items to return.
     * @return A list of recently added bookmarks.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun getRecentBookmarks(limit: Int): List<BookmarkItem>

    /**
     * Counts the number of bookmark items in the bookmark trees under the specified GUIDs.

     * @param guids The guids of folders to query.
     * @return Count of all bookmark items (ie, not folders or separators) in all specified folders recursively.
     * Empty folders, non-existing GUIDs and non-existing items will return zero.
     * The result is implementation dependant if the trees overlap.
     *
     * @throws OperationInterrupted if this database implements [InterruptibleConnection] and
     * has its `interrupt()` method called on another thread.
     */
    fun countBookmarksInTrees(guids: List<Guid>): UInt
}

/**
 * The methods provided by a bookmarks connection with write capabilities.
 */
interface WritableBookmarksConnection : ReadableBookmarksConnection {

    /**
     * Delete the bookmark with the provided GUID.
     *
     * If the requested bookmark is a folder, all children of
     * bookmark are deleted as well, recursively.
     *
     * @param guid The GUID of the bookmark to delete
     * @return Whether or not the bookmark existed.
     *
     * @throws CannotUpdateRoot If `guid` refers to a bookmark root.
     */
    fun deleteBookmarkNode(guid: Guid): Boolean

    /**
     * Delete all bookmarks without affecting history
     *
     */
    fun deleteAllBookmarks()

    /**
     * Create a bookmark folder, returning its guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param title The title of the folder.
     * @param position The index where to insert the record inside its parent.
     * If not provided, this item will be appended. If the position is outside
     * the range of positions currently occupied by children in this folder,
     * it is first constrained to be within that range.
     * @return The GUID of the newly inserted bookmark folder.
     *
     * @throws CannotUpdateRoot If `parentGUID` is the [BookmarkRoot.Root] (e.g. "root________")
     * @throws UnknownBookmarkItem If `parentGUID` does not refer to to a known bookmark.
     * @throws InvalidParent If `parentGUID` does not refer to a folder node.
     */
    fun createFolder(
        parentGUID: Guid,
        title: String,
        position: UInt? = null,
    ): Guid

    /**
     * Create a bookmark separator, returning its guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param position The index where to insert the record inside its parent.
     * If not provided, this item will be appended. If the position is outside
     * the range of positions currently occupied by children in this folder,
     * it is first constrained to be within that range.
     * @return The GUID of the newly inserted bookmark separator.
     *
     * @throws CannotUpdateRoot If `parentGUID` is the [BookmarkRoot.Root] (e.g. "root________")
     * @throws UnknownBookmarkItem If `parentGUID` does not refer to to a known bookmark.
     * @throws InvalidParent If `parentGUID` does not refer to a folder node.
     */
    fun createSeparator(
        parentGUID: Guid,
        position: UInt? = null,
    ): Guid

    /**
     * Create a bookmark item, returning its guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param url The URL to bookmark
     * @param title The title of the new bookmark.
     * @param position The index where to insert the record inside its parent.
     * If not provided, this item will be appended. If the position is outside
     * the range of positions currently occupied by children in this folder,
     * it is first constrained to be within that range.
     * @return The GUID of the newly inserted bookmark item.
     *
     * @throws CannotUpdateRoot If `parentGUID` is the [BookmarkRoot.Root] (e.g. "root________")
     * @throws UnknownBookmarkItem If `parentGUID` does not refer to to a known bookmark.
     * @throws InvalidParent If `parentGUID` does not refer to a folder node.
     * @throws UrlParseFailed If `url` does not refer to a valid URL.
     * @throws UrlTooLong if `url` exceeds the maximum length of 65536 bytes (when encoded)
     */
    fun createBookmarkItem(
        parentGUID: Guid,
        url: Url,
        title: String,
        position: UInt? = null,
    ): Guid

    /**
     * Update a bookmark to the provided info.
     *
     * @param guid GUID of the bookmark to update
     * @param parentGuid The new parent guid for the listed bookmark.
     * @param position The new position for the listed bookmark.
     * @param title The new title for the listed bookmark.
     * @param url The new url the listed bookmark.
     *
     * @throws InvalidBookmarkUpdate If the change requested is impossible given the
     * type of the item in the DB. For example, on attempts to update the title of a separator.
     * @throws CannotUpdateRoot If `guid` is a bookmark root, or `info.parentGUID`
     * is provided, and is [BookmarkRoot.Root] (e.g. "root________")
     * @throws UnknownBookmarkItem If `guid` or `info.parentGUID` (if specified) does not refer to
     * a known bookmark.
     * @throws InvalidParent If `info.parentGUID` is specified, but does not refer to a
     * folder node.
     */
    fun updateBookmark(guid: Guid, parentGuid: Guid?, position: UInt?, title: String?, url: Url?)
}
