/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 Represents a history metadata record, which describes metadata for a history visit, such as metadata
 about the page itself as well as metadata about how the page was opened.
 */
public struct HistoryMetadata {
    public let guid: String?
    public let url: String
    public let title: String?
    public let createdAt: Int64
    public let updatedAt: Int64
    public let totalViewTime: Int32
    public let searchTerm: String?
    public let isMedia: Bool
    public let parentUrl: String?
}

internal func unpackMetadataProtobuf(msg: MsgTypes_HistoryMetadata) -> HistoryMetadata {
    // Protobuf doesn't support passing around `null` value, so these get converted to some defaults
    // as they go from Rust to Swift. E.g. an empty string in place of a `null`.
    // Convert them back to nils here.
    let meta = HistoryMetadata(
        guid: msg.guid,
        url: msg.url,
        title: !msg.title.isEmpty ? msg.title : nil,
        createdAt: msg.createdAt,
        updatedAt: msg.updatedAt,
        totalViewTime: msg.totalViewTime,
        searchTerm: !msg.searchTerm.isEmpty ? msg.searchTerm : nil,
        isMedia: msg.isMedia,
        parentUrl: !msg.parentURL.isEmpty ? msg.parentURL : nil
    )

    return meta
}

internal func unpackMetadataListProtobuf(msg: MsgTypes_HistoryMetadataList) -> [HistoryMetadata] {
    return msg.metadata.map { node in
        unpackMetadataProtobuf(msg: node)
    }
}
