/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

/**
 Represents a document type of a page.
 */
public enum DocumentType: Int32, Codable {
    case regular = 0
    case media = 1

    public init?(value: Int32) {
        self.init(rawValue: value)
    }
}

/**
 Represents a set of properties which uniquely identify a history metadata. In database terms this is a compound key.
 */
public struct HistoryMetadataKey: Codable {
    public let url: String
    public let searchTerm: String?
    public let referrerUrl: String?

    public init(url: String, searchTerm: String?, referrerUrl: String?) {
        self.url = url
        self.searchTerm = searchTerm
        self.referrerUrl = referrerUrl
    }
}

/**
 Represents an observation about a `HistoryMetadataKey`.
 */
public struct HistoryMetadataObservation: Codable {
    var titleObservation: String? = nil
    var viewTimeObservation: Int32? = nil
    var documentTypeObservation: DocumentType? = nil
    
    public init(titleObservation: String?, viewTimeObservation: Int32?, documentTypeObservation: DocumentType?) {
        self.titleObservation = titleObservation
        self.viewTimeObservation = viewTimeObservation
        self.documentTypeObservation = documentTypeObservation
    }
}

/**
 Represents a history metadata record, which describes metadata for a history visit, such as metadata
 about the page itself as well as metadata about how the page was opened.
 */
public struct HistoryMetadata {
    public let key: HistoryMetadataKey
    public let title: String?
    public let createdAt: Int64
    public let updatedAt: Int64
    public let totalViewTime: Int32
    public let documentType: DocumentType
}

internal func unpackMetadataProtobuf(msg: MsgTypes_HistoryMetadata) -> HistoryMetadata {
    // Protobuf doesn't support passing around `null` value, so these get converted to some defaults
    // as they go from Rust to Swift. E.g. an empty string in place of a `null`.
    // Convert them back to nils here.
    return HistoryMetadata(
        key: HistoryMetadataKey(
            url: msg.url,
            searchTerm: !msg.searchTerm.isEmpty ? msg.searchTerm : nil,
            referrerUrl: !msg.referrerURL.isEmpty ? msg.referrerURL : nil
        ),
        title: !msg.title.isEmpty ? msg.title : nil,
        createdAt: msg.createdAt,
        updatedAt: msg.updatedAt,
        totalViewTime: msg.totalViewTime,
        documentType: DocumentType(rawValue: msg.documentType) ?? DocumentType.regular
    )
}

internal func unpackMetadataListProtobuf(msg: MsgTypes_HistoryMetadataList) -> [HistoryMetadata] {
    return msg.metadata.map { node in
        unpackMetadataProtobuf(msg: node)
    }
}
