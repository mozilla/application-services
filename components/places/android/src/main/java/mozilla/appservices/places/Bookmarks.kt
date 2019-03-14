/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

package mozilla.appservices.places

import java.lang.RuntimeException


/**
 * Enumeration of the ids of the roots of the bookmarks tree.
 *
 * There are 5 "roots" in the bookmark tree. The actual root
 * (which have no parent), and it's 4 children (which have the
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
 * Enumeration of the type of a bookmark item.
 *
 * Must match BookmarkType in the Rust code.
 */
enum class BookmarkType(val value: Int) {
    Bookmark(1),
    Folder(2),
    Separator(3),
}

/**
 * An interface defining the set of fields common to all nodes
 * in the bookmark tree.
 */
sealed class BookmarkTreeNode {
    /**
     * The type of this bookmark.
     */
    abstract val type: BookmarkType

    /**
     * The guid of this record. Bookmark guids are always 12 characters in the url-safe
     * base64 character set.
     */
    abstract val guid: String

    /**
     * Creation time, in milliseconds since the unix epoch.
     *
     * May not be a local timestamp.
     */
    abstract val dateAdded: Long

    /**
     * Last modification time, in milliseconds since the unix epoch.
     *
     * May not be a local timestamp.
     */
    abstract val lastModified: Long

    /**
     * The guid of this record's parent. It should only be null for
     * [BookmarkRoot.Root].
     */
    abstract val parentGUID: String?

    /**
     * The (0-based) position of this record within it's parent.
     */
    abstract val position: Int
}

/**
 * A bookmark tree node that actually represents a bookmark.
 *
 * It's type is always [BookmarkType.Bookmark], and it has a `title `and `url`
 * in addition to the fields defined by [BookmarkTreeNode].
 */

data class BookmarkItem(
        override val guid: String,
        override val dateAdded: Long,
        override val lastModified: Long,
        override val parentGUID: String?,
        override val position: Int,

        /**
         * The URL of this bookmark.
         */
        val url: String,

        /**
         * The title of the bookmark, if any was provided.
         */
        val title: String?
) : BookmarkTreeNode() {
    override val type get() = BookmarkType.Bookmark
}

/**
 * A bookmark which is a folder.
 *
 * It's type is always [BookmarkType.Folder], and it has a `title`,
 * a list of `childGUIDs`, and possibly a list of `children` in
 * addition to those defined by [BookmarkTreeNode].
 */
data class BookmarkFolder(
        override val guid: String,
        override val dateAdded: Long,
        override val lastModified: Long,
        override val parentGUID: String?,
        override val position: Int,

        /**
         * The title of this bookmark folder, if any was provided.
         */
        val title: String?,

        /**
         * The GUIDs of this folder's list of children.
         */
        val childGUIDs: List<String>,

        /**
         * If this node was returned the [ReadableBookmarksConnection.getBookmarksTree]
         * method, then this should have the list of children.
         */
        val children: List<BookmarkTreeNode>?

) : BookmarkTreeNode() {
    override val type get() = BookmarkType.Folder
}

/**
 * A bookmark which is a separator.
 *
 * It's type is always [BookmarkType.Separator], and it has no fields
 * besides those defined by [BookmarkTreeNode].
 */
data class BookmarkSeparator(
        override val guid: String,
        override val dateAdded: Long,
        override val lastModified: Long,
        override val parentGUID: String?,
        override val position: Int
) : BookmarkTreeNode() {
    override val type get() = BookmarkType.Separator
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
     * @param recursive Whether or not to return more than a single
     *                  level of children for folders. If false, then
     *                  any folders which are children of the requested
     *                  node will *only* have their `childGUIDs`
     *                  populated, and *not* their `children`.
     *
     * @return The bookmarks tree starting at `rootGUID`, or null if the provided
     *         id didn't refer to a known bookmark item.
     */
    fun getBookmarksTree(rootGUID: String, recursive: Boolean): BookmarkTreeNode?

    /**
     * Returns the information about the bookmark with the provided id. This differs from
     * `getBookmarksTree` in that it does not populate the `children` list if `guid` refers
     * to a folder (only it's `childGUIDs` list).
     *
     * @param guid the guid of the bookmark to fetch.
     * @return The bookmark node, or null if the provided
     *         guid didn't refer to a known bookmark item.
     */
    fun getBookmark(guid: String): BookmarkTreeNode?

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
     */
    fun getBookmarksWithURL(url: String): List<BookmarkItem>

    /**
     * Returns the list of bookmarks that match the provided search string.
     *
     * The order of the results is unspecified.
     *
     * @param query The search query
     * @param limit The maximum number of items to return.
     * @return A list of bookmarks where either the URL or the title
     *         contain a word (e.g. space separated item) from the
     *         query.
     */
    fun searchBookmarks(query: String, limit: Int): List<BookmarkItem>
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
     */
    fun deleteBookmarkNode(guid: String): Boolean

    /**
     * Create a bookmark folder, returning it's guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param title The title of the folder.
     * @param position The index where to insert the record inside
     *                 it's parent. If not provided, this item will
     *                 be appended.
     * @return The GUID of the newly inserted bookmark folder.
     */
    fun createFolder(
            parentGUID: String,
            title: String,
            position: Int? = null
    ): String

    /**
     * Create a bookmark separator, returning it's guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param position The index where to insert the record inside
     *                 it's parent. If not provided, this item will
     *                 be appended.
     * @return The GUID of the newly inserted bookmark separator.
     */
    fun createSeparator(
            parentGUID: String,
            position: Int? = null
    ): String

    /**
     * Create a bookmark item, returning it's guid.
     *
     * @param parentGUID The GUID of the (soon to be) parent of this bookmark.
     * @param url The URL to bookmark
     * @param title The title of the new bookmark, if any.
     * @param position The index where to insert the record inside
     *                 it's parent. If not provided, this item will
     *                 be appended.
     * @return The GUID of the newly inserted bookmark item.
     */
    fun createBookmarkItem(
            parentGUID: String,
            url: String,
            title: String?,
            position: Int? = null
    ): String

    /**
     * Update a bookmark to the provided info.
     *
     * @param guid Guid of the bookmark to update
     * @param info The changes to make to the listed bookmark.
     */
    fun updateBookmark(guid: String, info: BookmarkUpdateInfo)
}

/**
 * Information describing the changes to make in order to update a bookmark.
 */
data class BookmarkUpdateInfo(
        /**
         * If the record should be moved to another folder, the guid
         * of the folder it should be moved to. Interacts with
         * `position`, see it's documentation for details.
         */
        val parentGUID: String? = null,

        /**
         * If the record should be moved, the 0-based index where it
         * should be moved to. Interacts with `parentGUID` as follows:,
         *
         * - If `parentGUID` is not provided and `position` is, we treat this
         *   a move within the same folder.
         *
         * - If `parentGUID` and `position` are both provided, we treat this as
         *   a move to / within that folder, and we insert at the requested
         *   position.
         *
         * - If `position` is not provided (and `parentGUID` is) then it's
         *   treated as a move the end of that folder.
         */
        val position: Int? = null,

        /**
         * For nodes of type [BookmarkType.Bookmark] and [BookmarkType.Folder],
         * a string specifying the new title of the bookmark node.
         */
        val title: String? = null,

        /**
         * For nodes of type [BookmarkType.Bookmark], a string specifying
         * the new url of the bookmark node.
         */
        val url: String? = null
) {

    internal fun toProtobuf(guid: String): MsgTypes.BookmarkNode {
        val builder = MsgTypes.BookmarkNode.newBuilder()
        builder.setGuid(guid)
        this.position?.let { builder.setPosition(it) }
        this.parentGUID?.let { builder.setParentGuid(it) }
        this.title?.let { builder.setTitle(it) }
        this.url?.let { builder.setUrl(it) }
        return builder.build()
    }
}

/**
 * Turn the protobuf rust passes us into a BookmarkTreeNode.
 *
 * Note that we have no way to determine empty lists and lists that weren't provided, so we pass
 * in what we.
 * expect as a boolean flag (shouldHaveChildNodes).
 */
internal fun unpackProtobuf(msg: MsgTypes.BookmarkNode): BookmarkTreeNode {
    val guid = msg.guid
    val parentGUID = msg.parentGuid
    val position = msg.position
    val dateAdded = msg.dateAdded
    val lastModified = msg.lastModified
    val type = msg.nodeType
    val title = if (msg.hasTitle()) { msg.title } else { null }
    val shouldHaveChildNodes = if (msg.hasHaveChildNodes()) { msg.haveChildNodes } else { false }
    when (type) {

        BookmarkType.Bookmark.value -> {
            return BookmarkItem(
                    guid = guid,
                    parentGUID = parentGUID,
                    position = position,
                    dateAdded = dateAdded,
                    lastModified = lastModified,
                    title = title,
                    url = msg.url
            )
        }

        BookmarkType.Separator.value -> {
            return BookmarkSeparator(
                    guid = guid,
                    parentGUID = parentGUID,
                    position = position,
                    dateAdded = dateAdded,
                    lastModified = lastModified
            )
        }

        BookmarkType.Folder.value -> {
            val childNodes: List<BookmarkTreeNode> = msg.childNodesList.map {
                child -> unpackProtobuf(child)
            }
            var childGuids = msg.childGuidsList

            // If we got child nodes instead of guids, use the nodes to get the guids.
            if (childGuids.isEmpty() && childNodes.isNotEmpty()) {
                childGuids = childNodes.map { child -> child.guid }
            }

            return BookmarkFolder(
                    guid = guid,
                    parentGUID = parentGUID,
                    position = position,
                    dateAdded = dateAdded,
                    lastModified = lastModified,
                    title = title,
                    childGUIDs = childGuids,
                    children = if (shouldHaveChildNodes) { childNodes } else { null }
            )
        }

        else -> {
            // Should never happen
            throw RuntimeException("Rust passed in an illegal bookmark type $type")
        }
    }
}

// Unpack results from getBookmarksWithURL and searchBookmarks. Both of these can only return
// BookmarkItems, so we just do the cast inside the mapper.
internal fun unpackProtobufItemList(msg: MsgTypes.BookmarkNodeList): List<BookmarkItem> {
    return msg.nodesList.map { unpackProtobuf(it) as BookmarkItem }
}

