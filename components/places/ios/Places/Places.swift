/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import os.log

internal typealias ApiHandle = UInt64
internal typealias ConnHandle = UInt64

/**
 * This is something like a places connection manager. It primarially exists to
 * ensure that only a single write connection is active at once.
 *
 * If it helps, you can think of this as something like a connection pool
 * (although it does not actually perform any pooling).
 */
public class PlacesApi {
    private let handle: ApiHandle
    private let writeConn: PlacesWriteConn
    fileprivate let queue = DispatchQueue(label: "com.mozilla.places.api")

    /**
     * Initialize a PlacesApi
     *
     * - Parameter path: an absolute path to a file that will be used for the internal database.
     *
     * - Parameter encryptionKey: an optional key used for encrypting/decrypting data stored
     *                            in the internal database. If omitted, data will be stored
     *                            as plaintext.
     *
     * - Throws: `PlacesError` if initializing the database failed.
     */
    public init(path: String, encryptionKey: String? = nil) throws {
        let handle = try PlacesError.unwrap { error in
            places_api_new(path, encryptionKey, error)
        }
        self.handle = handle
        do {
            let writeHandle = try PlacesError.unwrap { error in
                places_connection_new(handle, Int32(PlacesConn_ReadWrite), error)
            }
            self.writeConn = PlacesWriteConn(handle: writeHandle)
            self.writeConn.api = self
        } catch let e {
            // We failed to open the write connection, even though the
            // API was opened. This is... strange, but possible.
            // Anyway, we want to clean up our API if this happens.
            //
            // If closing the API fails, it's probably caused by the same underlying
            // problem as whatever made us fail to open the write connection, so we'd
            // rather use the first error, since it's hopefully more descriptive.
            PlacesError.unwrapOrLog { error in
                places_api_destroy(handle, error)
            }
            throw e
        }
    }

    deinit {
        // Note: we shouldn't need to queue.sync with our queue in deinit (no more references
        // exist to us), however we still need to sync with the write conn's queue, since it
        // could still be in use.

        self.writeConn.queue.sync {
            // If the writer is still around (it should be), return it to the api.
            let writeHandle = self.writeConn.takeHandle()
            if writeHandle != 0 {
                PlacesError.unwrapOrLog { error in
                    places_api_return_write_conn(self.handle, writeHandle, error)
                }
            }
        }

        PlacesError.unwrapOrLog { error in
            places_api_destroy(self.handle, error)
        }
    }

    /**
     * Open a new reader connection.
     *
     * - Throws: `PlacesError` if a connection could not be opened.
     */
    func openReader() throws -> PlacesReadConn {
        return try queue.sync {
            let h = try PlacesError.unwrap { error in
                places_connection_new(handle, Int32(PlacesConn_ReadOnly), error)
            }
            return PlacesReadConn(handle: h, api: self)
        }
    }

    /**
     * Get the writer connection.
     *
     * - Note: There is only ever a single writer connection,
     *         and it's opened when the database is constructed,
     *         so this function does not throw
     */
    func getWriter() -> PlacesWriteConn {
        return queue.sync {
            self.writeConn
        }
    }
    

}

/**
 * A read-only connection to the places database.
 */
public class PlacesReadConn {
    fileprivate let queue = DispatchQueue(label: "com.mozilla.places.conn")
    fileprivate var handle: ConnHandle;
    fileprivate weak var api: PlacesApi?

    fileprivate init(handle: ConnHandle, api: PlacesApi? = nil) {
        self.handle = handle
        self.api = api
    }

    // Note: caller synchronizes!
    fileprivate func checkApi() throws {
        if api == nil {
            throw PlacesError.connUseAfterApiClosed
        }
    }

    // Note: caller synchronizes!
    fileprivate func takeHandle() -> ConnHandle {
        let handle = self.handle
        self.handle = 0
        return handle
    }

    deinit {
        // Note: don't need to queue.sync in deinit -- no more references exist to us.
        let handle = self.takeHandle()
        if handle != 0 {
            // In practice this can only fail if the rust code panics, which for this
            // function would be quite bad.
            try! PlacesError.tryUnwrap({ err in
                places_connection_destroy(handle, err)
            })
        }
    }

    /**
     * Returns the bookmark subtree rooted at `rootGUID`.
     *
     * This differs from `getBookmark` in that it populates folder children
     * recursively (specifically, any `BookmarkFolder`s in the returned value
     * will have their `children` list populated, and not just `childGUIDs`.
     *
     * - Parameter rootGUID: the GUID where to start the tree. Defaults to
     *                       `BookmarkRoot.RootGUID`, e.g. fetching the
     *                       entire bookmarks tree.
     *
     * - Returns: The fully populated bookmarks tree starting from `rootGUID`,
     *            or null if the provided guid didn't refer to a known
     *            bookmark item.
     */
    func getBookmarksTree(rootGUID: String = BookmarkRoots.RootGUID) throws -> BookmarkNode? {
        return try queue.sync {
            try self.checkApi()
            let buffer = try PlacesError.unwrap { error in
                bookmarks_get_tree(self.handle, rootGUID, error)
            }
            if buffer.data == nil {
                return nil
            }
            defer { places_destroy_bytebuffer(buffer) }
            // This should never fail, since we encoded it on the other side with Rust,
            // should we use `try! instead?
            let msg = try MsgTypes_BookmarkNode(serializedData: Data(placesRustBuffer: buffer))
            return unpackProtobuf(msg: msg, expectChildren: true)
        }
    }

    /**
     * Returns the information about the bookmark with the provided id.
     *
     * This differs from `getBookmarksTree` in that it does not populate the `children` list
     * if `guid` refers to a folder (However, it's `childGUIDs` list will be
     * populated).
     *
     * - Parameter guid: the guid of the bookmark to fetch.
     *
     * - Returns: The bookmark node, or null if the provided guid didn't refer to a
     *            known bookmark item.
     */
    func getBookmark(guid: String) throws -> BookmarkNode? {
        return try queue.sync {
            try self.checkApi()
            let buffer = try PlacesError.unwrap { error in
                bookmarks_get_by_guid(self.handle, guid, error)
            }
            if buffer.data == nil {
                return nil
            }
            defer { places_destroy_bytebuffer(buffer) }
            // Should this be `try!`?
            let msg = try MsgTypes_BookmarkNode(serializedData: Data(placesRustBuffer: buffer))
            return unpackProtobuf(msg: msg, expectChildren: false)
        }
    }

    /**
     * Returns the list of bookmarks with the provided URL.
     *
     * - Note: If the URL is not percent-encoded/punycoded, that will be performed
     *         internally, and so the returned bookmarks may not have an identical
     *         URL to the one passed in, however, it will be the same according to
     *         https://url.spec.whatwg.org
     *
     * - Parameter url: The url to search for.
     *
     * - Returns: A list of bookmarks that have the requested URL.
     */
    func getBookmarksWithURL(url: String) throws -> [BookmarkNode] {
        return try queue.sync {
            try self.checkApi()
            let buffer = try PlacesError.unwrap { error in
                bookmarks_get_all_with_url(self.handle, url, error)
            }
            defer { places_destroy_bytebuffer(buffer) }
            // Should this be `try!`?
            let msg = try MsgTypes_BookmarkNodeList(serializedData: Data(placesRustBuffer: buffer))
            return unpackProtobufList(msg: msg)
        }
    }

}

/**
 * A read-write connection to the places database.
 */
public class PlacesWriteConn : PlacesReadConn {

    /**
     * Delete the bookmark with the provided GUID.
     *
     * If the requested bookmark is a folder, all children of
     * bookmark are deleted as well, recursively.
     *
     * - Parameter guid: The GUID of the bookmark to delete
     *
     * - Returns: Whether or not the bookmark existed.
     */
    func deleteBookmark(guid: String) throws -> Bool {
        return try queue.sync {
            try self.checkApi()
            let resByte = try PlacesError.unwrap { error in
                bookmarks_delete(self.handle, guid, error)
            }
            return resByte != 0
        }
    }

    /**
     * Create a bookmark folder, returning it's guid.
     *
     * - Parameter parentGUID: The GUID of the (soon to be) parent of this bookmark.
     *
     * - Parameter title: The title of the folder.
     *
     * - Parameter position: The index where to insert the record inside
     *                       it's parent. If not provided, this item will
     *                       be appended.
     *
     * - Returns: The GUID of the newly inserted bookmark folder.
     */
    func createFolder(parentGUID: String, title: String, position: UInt32? = nil) throws -> String {
        return try queue.sync {
            try self.checkApi()
            var msg = insertionMsg(type: .folder, parentGUID: parentGUID, position: position)
            msg.title = title
            return try doInsert(msg: msg)
        }
    }


    /**
     * Create a bookmark separator, returning it's guid.
     *
     * - Parameter parentGUID: The GUID of the (soon to be) parent of this bookmark.
     *
     * - Parameter position: The index where to insert the record inside
     *                       it's parent. If not provided, this item will
     *                       be appended.
     *
     * - Returns: The GUID of the newly inserted bookmark separator.
     */
    func createSeparator(parentGUID: String, position: UInt32? = nil) throws -> String {
        return try queue.sync {
            try self.checkApi()
            let msg = insertionMsg(type: .separator, parentGUID: parentGUID, position: position)
            return try doInsert(msg: msg)
        }
    }

    /**
     * Create a bookmark item, returning it's guid.
     *
     * - Parameter parentGUID: The GUID of the (soon to be) parent of this bookmark.
     *
     * - Parameter position: The index where to insert the record inside
     *                       it's parent. If not provided, this item will
     *                       be appended.
     *
     * - Parameter url: The URL to bookmark
     *
     * - Parameter title: The title of the new bookmark, if any.
     *
     * - Returns: The GUID of the newly inserted bookmark item.
     */
    func createBookmark(parentGUID: String, url: String, title: String?, position: UInt32? = nil) throws -> String {
        return try queue.sync {
            try self.checkApi()
            var msg = insertionMsg(type: .bookmark, parentGUID: parentGUID, position: position)
            msg.url = url
            if let t = title {
                msg.title = t
            }
            return try doInsert(msg: msg)
        }
    }

    /**
     * Update a bookmark to the provided info.
     *
     * - Parameters:
     *     - guid: Guid of the bookmark to update
     *
     *     - type: The type of the bookmark you wish to update.
     *             Note that it is an error to attempt to change the type
     *             of a bookmark.
     *
     *     - parentGUID: If the record should be moved to another folder, the guid
     *                   of the folder it should be moved to. Interacts with
     *                   `position`, see the note below for details.
     *
     *     - position: If the record should be moved, the 0-based index where it
     *                 should be moved to. Interacts with `parentGUID`, see the note
     *                 below for details
     *
     *     - title: If the record is a `BookmarkNodeType.bookmark` or a `BookmarkNodeType.folder`,
     *              and it's title should be changed, then the new value of the title.
     *
     *     - url: If the record is a `BookmarkNodeType.bookmark` node, and it's `url`
     *            should be changed, then the new value for the url.
     *
     * - Note: The `parentGUID` and `position` parameters interact with eachother
     *   as follows:
     *
     *     - If `parentGUID` is not provided and `position` is, we treat this
     *       a move within the same folder.
     *
     *     - If `parentGUID` and `position` are both provided, we treat this as
     *       a move to / within that folder, and we insert at the requested
     *       position.
     *
     *     - If `position` is not provided (and `parentGUID` is) then it's
     *       treated as a move the end of that folder.
     */
    func updateBookmark(guid: String,
                        type: BookmarkNodeType,
                        parentGUID: String? = nil,
                        position: UInt32? = nil,
                        title: String? = nil,
                        url: String? = nil) throws
    {
        try queue.sync {
            try self.checkApi()
            var msg = MsgTypes_BookmarkNode()
            msg.nodeType = type.rawValue
            msg.guid = guid
            if let parent = parentGUID {
                msg.parentGuid = parent
            }
            if let pos = position {
                msg.position = pos
            }
            if let t = title {
                msg.title = t
            }
            if let u = url {
                msg.url = u
            }
            let data = try! msg.serializedData()
            let size = Int32(data.count)
            try data.withUnsafeBytes { (bytes: UnsafePointer<UInt8>) in
                try PlacesError.unwrap { error in
                    bookmarks_update(self.handle, bytes, size, error)
                }
            }
        }
    }

    // Helper for the various creation functions.
    // Note: Caller synchronizes
    private func doInsert(msg: MsgTypes_BookmarkNode) throws -> String {
        // This can only fail if we failed to set the `type` of the msg
        let data = try! msg.serializedData()
        let size = Int32(data.count)
        return try data.withUnsafeBytes { (bytes: UnsafePointer<UInt8>) -> String in
            let idStr = try PlacesError.unwrap { error in
                bookmarks_insert(self.handle, bytes, size, error)
            }
            return String(freeingPlacesString: idStr)
        }
    }

    // Remove the boilerplate common for all insertion messages
    private func insertionMsg(type: BookmarkNodeType, parentGUID: String, position: UInt32?) -> MsgTypes_BookmarkNode {
        var msg = MsgTypes_BookmarkNode()
        msg.nodeType = type.rawValue
        msg.parentGuid = parentGUID
        if let pos = position {
            msg.position = pos
        }
        return msg
    }
}

