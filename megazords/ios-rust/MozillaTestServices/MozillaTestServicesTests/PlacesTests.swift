@testable import MozillaTestServices
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */
import XCTest
// some utility functions for the test code

func dynCmp<T: Equatable>(_ optVal: T?, _ optDynVal: Any?) -> Bool {
    guard let dynVal = optDynVal else {
        // no requierment given, all is fine
        return true
    }
    guard let val = optVal else {
        // a requirement was given but it's missing on our end
        return false
    }
    guard let typedV = dynVal as? T else {
        // a requirement was given but the type is wrong.
        return false
    }
    return typedV == val
}

// It's a pain to pass `BookmarkNodeType.separator` into the JSON,
// so this converts strings to it if a string was passed in. Returns
// nil if the arg is nil.
func typeFromAny(_ any: Any?) -> BookmarkNodeType? {
    guard let ty = any else {
        return nil
    }
    if let result = ty as? BookmarkNodeType {
        return result
    }
    let str = ty as! String
    switch str {
    case "separator":
        return .separator
    case "folder":
        return .folder
    case "bookmark":
        return .bookmark
    default:
        // this probably means we have a typo in our test code
        XCTFail("Test specified invalid type string: \(str)")
        return nil // not reached, AFAIK
    }
}

enum CheckChildren {
    // Check whatever is provided
    case full
    // Only childGUIDs should be present
    case onlyGUIDs
    // after 1 level of full checking, only childGUIDs should be provided
    // (for trees from getBookmarksTree(recursive:false))
    case onlyGUIDsInChildren
}

// similar assert_json_tree from our rust code.
func checkTree(_ n: BookmarkNodeData, _ want: [String: Any], checkChildren: CheckChildren = .full) {
    XCTAssert(n.parentGUID != nil || n.guid == BookmarkRoots.RootGUID)

    XCTAssert(dynCmp(n.guid, want["guid"]))
    XCTAssert(dynCmp(n.type, typeFromAny(want["type"])))

    switch n.type {
    case .separator:
        XCTAssert(n is BookmarkSeparatorData)
    case .bookmark:
        XCTAssert(n is BookmarkItemData)
    case .folder:
        XCTAssert(n is BookmarkFolderData)
    }

    if let bn = n as? BookmarkItemData {
        XCTAssert(dynCmp(bn.url, want["url"]))
        XCTAssert(dynCmp(bn.title, want["title"]))
    } else {
        XCTAssertNil(want["url"])
    }

    if let fn = n as? BookmarkFolderData {
        if checkChildren == .onlyGUIDs {
            XCTAssertNil(fn.children)
            // Make sure it's not getting provided accidentally
            XCTAssertNil(want["children"])
        }
        if let wantedChildren = want["children"] as? [[String: Any]] {
            let children = fn.children!
            XCTAssertEqual(children.count, wantedChildren.count)
            let nextCheckChildren = checkChildren == .onlyGUIDsInChildren ? .onlyGUIDs : checkChildren
            // we need `i` for comparing position, or we'd just use zip().
            for i in 0 ..< children.count {
                let child = children[i]
                XCTAssertEqual(child.guid, fn.childGUIDs[i])
                XCTAssertEqual(child.parentGUID, fn.guid)
                XCTAssertEqual(Int(child.position), i)
                let wantChild = wantedChildren[i]
                checkTree(child, wantChild, checkChildren: nextCheckChildren)
            }
        }
        if let wantedGUIDs = want["childGUIDs"] as? [String] {
            XCTAssertEqual(wantedGUIDs, fn.childGUIDs)
        }
    } else {
        XCTAssertNil(want["children"])
        XCTAssertNil(want["childGUIDs"])
    }
}

var counterValue = 0
func counter() -> Int {
    counterValue += 1
    return counterValue
}

@discardableResult
func insertTree(_ db: PlacesWriteConnection, parent: Guid, tree: [String: Any]) -> String {
    let root = try! db.createFolder(parentGUID: parent, title: (tree["title"] as? String) ?? "folder \(counter())")
    for child in tree["children"] as! [[String: Any]] {
        switch typeFromAny(child["type"])! {
        case .separator:
            try! db.createSeparator(parentGUID: root)
        case .bookmark:
            let ctr = counter()
            let url = (child["url"] as? String) ?? "http://www.example.com/\(ctr)"
            try! db.createBookmark(parentGUID: root, url: url, title: child["title"] as? String)
        case .folder:
            insertTree(db, parent: root, tree: child)
        }
    }
    return root
}

let EmptyChildren: [[String: Any]] = []

let DummyTree0: [String: Any] = [
    "type": "folder",
    "title": "my favorite bookmarks",
    "children": [
        [
            "type": "bookmark",
            "url": "http://www.github.com/",
            "title": "github",
        ],
        [
            "type": "separator",
        ],
        [
            "type": "folder",
            "title": "cool folder",
            "children": [
                [
                    "type": "bookmark",
                    "title": "example0",
                    "url": "https://www.example0.com/",
                ],
                [
                    "type": "folder",
                    "title": "empty folder",
                    "children": EmptyChildren,
                ],
                [
                    "type": "bookmark",
                    "title": "example1",
                    "url": "https://www.example1.com/",
                ],
            ],
        ],
    ],
]

class PlacesTests: XCTestCase {
    // XXX: We don't clean up PlacesAPIs properly (issue 749), so
    // it's not great that we create a new one of these for each test!
    var api: PlacesAPI!

    override func setUp() {
        // This method is called before the invocation of each test method in the class.
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("testdb-\(UUID().uuidString).db")
        api = try! PlacesAPI(path: url.path)
    }

    override func tearDown() {
        // This method is called after the invocation of each test method in the class.
    }

    func testGetTree() {
        let db = api.getWriter()

        checkTree(try! db.getBookmarksTree(rootGUID: BookmarkRoots.RootGUID, recursive: true)!, [
            "guid": BookmarkRoots.RootGUID,
            "title": "root",
            "children": [
                [
                    "guid": BookmarkRoots.MenuFolderGUID,
                    "children": EmptyChildren,
                ],
                [
                    "guid": BookmarkRoots.ToolbarFolderGUID,
                    "children": EmptyChildren,
                ],
                [
                    "guid": BookmarkRoots.UnfiledFolderGUID,
                    "children": EmptyChildren,
                ],
                [
                    "guid": BookmarkRoots.MobileFolderGUID,
                    "children": EmptyChildren,
                ],
            ],
        ])

        insertTree(db, parent: BookmarkRoots.MenuFolderGUID, tree: DummyTree0)

        let got = try! db.getBookmarksTree(rootGUID: BookmarkRoots.MenuFolderGUID, recursive: true)!

        checkTree(got, [
            "guid": BookmarkRoots.MenuFolderGUID,
            "type": "folder",
            "children": [DummyTree0],
        ])

        // Check recursive: false
        let noGrandkids = try! db.getBookmarksTree(rootGUID: BookmarkRoots.MenuFolderGUID, recursive: false)! as! BookmarkFolderData

        let expectedChildGuids = ((got as! BookmarkFolderData).children![0] as! BookmarkFolderData).childGUIDs

        checkTree(noGrandkids, [
            "guid": BookmarkRoots.MenuFolderGUID,
            "type": "folder",
            "children": [
                [
                    "type": "folder",
                    "title": "my favorite bookmarks",
                    "childGUIDs": expectedChildGuids,
                ],
            ],
        ], checkChildren: .onlyGUIDsInChildren)
    }

    func testGetBookmark() {
        let db = api.getWriter()

        let newFolderGUID = insertTree(db, parent: BookmarkRoots.MenuFolderGUID, tree: DummyTree0)
        let sepGUID = try! db.createSeparator(parentGUID: BookmarkRoots.MenuFolderGUID)

        checkTree(try! db.getBookmark(guid: BookmarkRoots.MenuFolderGUID)!, [
            "guid": BookmarkRoots.MenuFolderGUID,
            "type": "folder",
            "childGUIDs": [newFolderGUID, sepGUID],
        ], checkChildren: .onlyGUIDs)
    }

    // MARK: history metadata tests

    func testHistoryMetadataBasics() {
        let beginning = Int64(Date().timeIntervalSince1970 * 1000)
        let db = api.getWriter()

        XCTAssertEqual(0, try! db.getHistoryMetadataSince(since: beginning).count)

        XCTAssertNil(try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org"))

        let metaKey1 = HistoryMetadataKey(
            url: "http://www.mozilla.org",
            searchTerm: nil,
            referrerUrl: nil
        )
        _ = try! db.noteHistoryMetadataObservationDocumentType(key: metaKey1, documentType: DocumentType.media)

        XCTAssertEqual(1, try! db.getHistoryMetadataSince(since: beginning).count)

        var dbMeta = try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org")
        XCTAssertNotNil(dbMeta)
        XCTAssertEqual("http://www.mozilla.org/", dbMeta!.url)
        XCTAssertEqual(nil, dbMeta!.title)
        XCTAssertEqual(nil, dbMeta!.referrerUrl)
        XCTAssertEqual(nil, dbMeta!.searchTerm)
        XCTAssertEqual(DocumentType.media, dbMeta!.documentType)
        XCTAssertEqual(0, dbMeta!.totalViewTime)

        XCTAssertEqual(1, try! db.getHistoryMetadataSince(since: beginning).count)

        _ = try! db.noteHistoryMetadataObservationViewTime(key: metaKey1, viewTime: 1337)
        dbMeta = try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org")
        XCTAssertNotNil(dbMeta)
        XCTAssertEqual("http://www.mozilla.org/", dbMeta!.url)
        XCTAssertEqual(nil, dbMeta!.title)
        XCTAssertEqual(nil, dbMeta!.referrerUrl)
        XCTAssertEqual(nil, dbMeta!.searchTerm)
        XCTAssertEqual(DocumentType.media, dbMeta!.documentType)
        XCTAssertEqual(1337, dbMeta!.totalViewTime)

        XCTAssertEqual(1, try! db.getHistoryMetadataSince(since: beginning).count)

        _ = try! db.noteHistoryMetadataObservationViewTime(key: metaKey1, viewTime: 3)
        dbMeta = try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org")
        XCTAssertEqual(1340, dbMeta!.totalViewTime)

        XCTAssertEqual(1, try! db.getHistoryMetadataSince(since: beginning).count)
        let afterLastMeta1Update = Int64(Date().timeIntervalSince1970 * 1000)

        let metaKey2 = HistoryMetadataKey(
            url: "http://www.mozilla.org/another/",
            searchTerm: "another firefox",
            referrerUrl: "https://www.google.com/search?client=firefox-b-d&q=another+firefox"
        )
        _ = try! db.noteHistoryMetadataObservationTitle(key: metaKey2, title: "some title")

        XCTAssertEqual(1, try! db.getHistoryMetadataSince(since: afterLastMeta1Update).count)
        XCTAssertEqual(2, try! db.getHistoryMetadataSince(since: beginning).count)

        var dbMeta2 = try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org/another/")
        XCTAssertNotNil(dbMeta2)
        XCTAssertEqual("http://www.mozilla.org/another/", dbMeta2!.url)
        XCTAssertEqual("some title", dbMeta2!.title)
        XCTAssertEqual("https://www.google.com/search?client=firefox-b-d&q=another+firefox", dbMeta2!.referrerUrl)
        XCTAssertEqual("another firefox", dbMeta2!.searchTerm)
        XCTAssertEqual(DocumentType.regular, dbMeta2!.documentType)
        XCTAssertEqual(0, dbMeta2!.totalViewTime)

        _ = try! db.noteHistoryMetadataObservationDocumentType(key: metaKey2, documentType: DocumentType.regular)
        _ = try! db.noteHistoryMetadataObservationTitle(key: metaKey2, title: "Some Title")
        _ = try! db.noteHistoryMetadataObservationViewTime(key: metaKey2, viewTime: 52345)
        dbMeta2 = try! db.getLatestHistoryMetadataForUrl(url: "http://www.mozilla.org/another/")
        XCTAssertNotNil(dbMeta2)
        XCTAssertEqual("http://www.mozilla.org/another/", dbMeta2!.url)
        XCTAssertEqual("some title", dbMeta2!.title) // NB: subsequent title updates are currently ignored
        XCTAssertEqual("https://www.google.com/search?client=firefox-b-d&q=another+firefox", dbMeta2!.referrerUrl)
        XCTAssertEqual("another firefox", dbMeta2!.searchTerm)
        XCTAssertEqual(DocumentType.regular, dbMeta2!.documentType)
        XCTAssertEqual(52345, dbMeta2!.totalViewTime)

        let afterLastMeta2Update = Int64(Date().timeIntervalSince1970 * 1000)

        XCTAssertEqual(2, try! db.getHistoryMetadataBetween(start: beginning, end: afterLastMeta2Update).count)
        XCTAssertEqual(1, try! db.getHistoryMetadataBetween(start: afterLastMeta1Update, end: afterLastMeta2Update).count)

        // by search term
        var list = try! db.queryHistoryMetadata(query: "firefox", limit: 100)
        XCTAssertEqual(1, list.count)
        // by url
        list = try! db.queryHistoryMetadata(query: "mozilla", limit: 100)
        XCTAssertEqual(2, list.count)
        // by title
        list = try! db.queryHistoryMetadata(query: "title", limit: 100)
        XCTAssertEqual(1, list.count)
        // by title
        list = try! db.queryHistoryMetadata(query: "Title", limit: 100)
        XCTAssertEqual(1, list.count)

        try! db.deleteHistoryMetadataOlderThan(olderThan: beginning)
        XCTAssertEqual(2, try! db.getHistoryMetadataSince(since: beginning).count)
        try! db.deleteHistoryMetadataOlderThan(olderThan: afterLastMeta1Update)
        list = try! db.getHistoryMetadataSince(since: beginning)
        XCTAssertEqual(1, list.count)
        XCTAssertEqual("http://www.mozilla.org/another/", list[0].url)
        try! db.deleteHistoryMetadataOlderThan(olderThan: afterLastMeta2Update)
        XCTAssertEqual(0, try! db.getHistoryMetadataSince(since: beginning).count)
    }

    // Due to the current hybrid approach of Uniffi for places, we're adding error test cases
    // To properly test uniffi & non-uniffi properly error propagate
    func testPlacesErrors() {
        let db = api.getWriter()

        // Testing a non-uniffi error
        do {
            _ = try db.updateBookmarkNode(guid: "123", parentGUID: "456")
            XCTFail("Call did not throw")
        } catch let caughtError as PlacesError {
            if case PlacesError.UnknownBookmarkItem = caughtError {
            } else {
                XCTFail("Not the correct error ")
            }
        } catch {
            XCTFail("Not a PlacesError")
        }

        // Testing a Uniffi-ed error
        do {
            _ = try db.noteHistoryMetadataObservation(observation: HistoryMetadataObservation(url: "http://www.[].com"))
            XCTFail("Call did not throw")
        } catch let caughtError as PlacesError {
            if case PlacesError.UrlParseFailed = caughtError {
            } else {
                XCTAssertEqual(caughtError.localizedDescription, "Error")
                XCTFail("Not the correct PlacesError")
            }
        } catch {
            let desc = error.localizedDescription
            XCTAssertEqual(desc, "Error")
            XCTFail("Not a PlacesError")
        }
    }
}
