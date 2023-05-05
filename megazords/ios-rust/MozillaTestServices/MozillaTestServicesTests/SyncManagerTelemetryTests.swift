/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

@testable import MozillaTestServices

import Glean
import XCTest

class SyncManagerTelemetryTests: XCTestCase {
    private var now: Int64 = 0

    override func setUp() {
        super.setUp()
        Glean.shared.resetGlean(clearStores: true)
        Glean.shared.enableTestingMode()
        now = Int64(Date().timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC
    }

    func testSendsLoginsHistoryAndGlobalPings() {
        var globalSyncUuid = UUID()
        let syncTelemetry = RustSyncTelemetryPing(version: 1,
                                                  uid: "abc123",
                                                  events: [],
                                                  syncs: [SyncInfo(at: now,
                                                                   took: 10000,
                                                                   engines: [EngineInfo(name: "passwords",
                                                                                        at: now,
                                                                                        took: 5000,
                                                                                        incoming: IncomingInfo(applied: 5,
                                                                                                               failed: 4,
                                                                                                               newFailed: 3,
                                                                                                               reconciled: 2),
                                                                                        outgoing: [OutgoingInfo(sent: 10,
                                                                                                                failed: 5),
                                                                                                   OutgoingInfo(sent: 4,
                                                                                                                failed: 2)],
                                                                                        failureReason: nil,
                                                                                        validation: nil),
                                                                             EngineInfo(name: "history",
                                                                                        at: now,
                                                                                        took: 5000,
                                                                                        incoming: IncomingInfo(applied: 5,
                                                                                                               failed: 4,
                                                                                                               newFailed: 3,
                                                                                                               reconciled: 2),
                                                                                        outgoing: [OutgoingInfo(sent: 10,
                                                                                                                failed: 5),
                                                                                                   OutgoingInfo(sent: 4,
                                                                                                                failed: 2)],
                                                                                        failureReason: nil,
                                                                                        validation: nil)],
                                                                   failureReason: FailureReason(name: FailureName.unknown,
                                                                                                message: "Synergies not aligned"))])

        func submitGlobalPing(_: NoReasonCodes?) {
            XCTAssertEqual("Synergies not aligned", GleanMetrics.Sync.failureReason["other"].testGetValue())
            XCTAssertNotNil(globalSyncUuid)
            XCTAssertEqual(globalSyncUuid, GleanMetrics.Sync.syncUuid.testGetValue("sync"))
        }

        func submitHistoryPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("history-sync")!
            XCTAssertEqual("abc123", GleanMetrics.HistorySync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.HistorySync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.HistorySync.finishedAt.testGetValue())
            XCTAssertEqual(now, Int64(GleanMetrics.HistorySync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 5, Int64(GleanMetrics.HistorySync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertEqual(5, GleanMetrics.HistorySync.incoming["applied"].testGetValue())
            XCTAssertEqual(7, GleanMetrics.HistorySync.incoming["failed_to_apply"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.HistorySync.incoming["reconciled"].testGetValue())
            XCTAssertEqual(14, GleanMetrics.HistorySync.outgoing["uploaded"].testGetValue())
            XCTAssertEqual(7, GleanMetrics.HistorySync.outgoing["failed_to_upload"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.HistorySync.outgoingBatches.testGetValue())
        }

        func submitLoginsPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("logins-sync")!
            XCTAssertEqual("abc123", GleanMetrics.LoginsSync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.LoginsSync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.LoginsSync.finishedAt.testGetValue())
            XCTAssertEqual(now, Int64(GleanMetrics.LoginsSync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 5, Int64(GleanMetrics.LoginsSync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertEqual(5, GleanMetrics.LoginsSync.incoming["applied"].testGetValue())
            XCTAssertEqual(7, GleanMetrics.LoginsSync.incoming["failed_to_apply"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.LoginsSync.incoming["reconciled"].testGetValue())
            XCTAssertEqual(14, GleanMetrics.LoginsSync.outgoing["uploaded"].testGetValue())
            XCTAssertEqual(7, GleanMetrics.LoginsSync.outgoing["failed_to_upload"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.LoginsSync.outgoingBatches.testGetValue())
        }

        try! processSyncTelemetry(syncTelemetry: syncTelemetry,
                                  submitGlobalPing: submitGlobalPing,
                                  submitHistoryPing: submitHistoryPing,
                                  submitLoginsPing: submitLoginsPing)
    }

    func testSendsHistoryAndGlobalPings() {
        var globalSyncUuid = UUID()
        let syncTelemetry = RustSyncTelemetryPing(version: 1,
                                                  uid: "abc123",
                                                  events: [],
                                                  syncs: [SyncInfo(at: now + 10,
                                                                   took: 5000,
                                                                   engines: [EngineInfo(name: "history",
                                                                                        at: now + 10,
                                                                                        took: 5000,
                                                                                        incoming: nil,
                                                                                        outgoing: [],
                                                                                        failureReason: nil,
                                                                                        validation: nil)],
                                                                   failureReason: nil)])

        func submitGlobalPing(_: NoReasonCodes?) {
            XCTAssertNil(GleanMetrics.Sync.failureReason["other"].testGetValue())
            XCTAssertNotNil(globalSyncUuid)
            XCTAssertEqual(globalSyncUuid, GleanMetrics.Sync.syncUuid.testGetValue("sync"))
        }

        func submitHistoryPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("history-sync")!
            XCTAssertEqual("abc123", GleanMetrics.HistorySync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.HistorySync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.HistorySync.finishedAt.testGetValue())
            XCTAssertEqual(now + 10, Int64(GleanMetrics.HistorySync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 15, Int64(GleanMetrics.HistorySync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertNil(GleanMetrics.HistorySync.incoming["applied"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.incoming["failed_to_apply"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.incoming["reconciled"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.outgoing["uploaded"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.outgoing["failed_to_upload"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.outgoingBatches.testGetValue())
        }

        try! processSyncTelemetry(syncTelemetry: syncTelemetry,
                                  submitGlobalPing: submitGlobalPing,
                                  submitHistoryPing: submitHistoryPing)
    }

    func testSendsBookmarksAndGlobalPings() {
        var globalSyncUuid = UUID()
        let syncTelemetry = RustSyncTelemetryPing(version: 1,
                                                  uid: "abc123",
                                                  events: [],
                                                  syncs: [SyncInfo(at: now + 20,
                                                                   took: 8000,
                                                                   engines: [EngineInfo(name: "bookmarks",
                                                                                        at: now + 25,
                                                                                        took: 6000,
                                                                                        incoming: nil,
                                                                                        outgoing: [OutgoingInfo(sent: 10, failed: 5)],
                                                                                        failureReason: nil,
                                                                                        validation: ValidationInfo(version: 2,
                                                                                                                   problems: [ProblemInfo(name: "missingParents",
                                                                                                                                          count: 5),
                                                                                                                              ProblemInfo(name: "missingChildren",
                                                                                                                                          count: 7)],
                                                                                                                   failureReason: nil))],
                                                                   failureReason: nil)])

        func submitGlobalPing(_: NoReasonCodes?) {
            XCTAssertNil(GleanMetrics.Sync.failureReason["other"].testGetValue())
            XCTAssertNotNil(globalSyncUuid)
            XCTAssertEqual(globalSyncUuid, GleanMetrics.Sync.syncUuid.testGetValue("sync"))
        }

        func submitBookmarksPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("bookmarks-sync")!
            XCTAssertEqual("abc123", GleanMetrics.BookmarksSync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.BookmarksSync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.BookmarksSync.finishedAt.testGetValue())
            XCTAssertEqual(now + 25, Int64(GleanMetrics.BookmarksSync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 31, Int64(GleanMetrics.BookmarksSync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertNil(GleanMetrics.BookmarksSync.incoming["applied"].testGetValue())
            XCTAssertNil(GleanMetrics.BookmarksSync.incoming["failed_to_apply"].testGetValue())
            XCTAssertNil(GleanMetrics.BookmarksSync.incoming["reconciled"].testGetValue())
            XCTAssertEqual(10, GleanMetrics.BookmarksSync.outgoing["uploaded"].testGetValue())
            XCTAssertEqual(5, GleanMetrics.BookmarksSync.outgoing["failed_to_upload"].testGetValue())
            XCTAssertEqual(1, GleanMetrics.BookmarksSync.outgoingBatches.testGetValue())
        }

        try! processSyncTelemetry(syncTelemetry: syncTelemetry,
                                  submitGlobalPing: submitGlobalPing,
                                  submitBookmarksPing: submitBookmarksPing)
    }

    func testSendsTabsCreditCardsAndGlobalPings() {
        var globalSyncUuid = UUID()
        let syncTelemetry = RustSyncTelemetryPing(version: 1,
                                                  uid: "abc123",
                                                  events: [],
                                                  syncs: [SyncInfo(at: now + 30,
                                                                   took: 10000,
                                                                   engines: [EngineInfo(name: "tabs",
                                                                                        at: now + 10,
                                                                                        took: 6000,
                                                                                        incoming: nil,
                                                                                        outgoing: [OutgoingInfo(sent: 8, failed: 2)],
                                                                                        failureReason: nil,
                                                                                        validation: nil),
                                                                             EngineInfo(name: "creditcards",
                                                                                        at: now + 15,
                                                                                        took: 4000,
                                                                                        incoming: IncomingInfo(applied: 3,
                                                                                                               failed: 1,
                                                                                                               newFailed: 1,
                                                                                                               reconciled: 0),
                                                                                        outgoing: [],
                                                                                        failureReason: nil,
                                                                                        validation: nil)],
                                                                   failureReason: nil)])

        func submitGlobalPing(_: NoReasonCodes?) {
            XCTAssertNil(GleanMetrics.Sync.failureReason["other"].testGetValue())
            XCTAssertNotNil(globalSyncUuid)
            XCTAssertEqual(globalSyncUuid, GleanMetrics.Sync.syncUuid.testGetValue("sync"))
        }

        func submitCreditCardsPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("creditcards-sync")!
            XCTAssertEqual("abc123", GleanMetrics.CreditcardsSync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.CreditcardsSync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.CreditcardsSync.finishedAt.testGetValue())
            XCTAssertEqual(now + 15, Int64(GleanMetrics.CreditcardsSync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 19, Int64(GleanMetrics.CreditcardsSync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertEqual(3, GleanMetrics.CreditcardsSync.incoming["applied"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.CreditcardsSync.incoming["failed_to_apply"].testGetValue())
            XCTAssertNil(GleanMetrics.CreditcardsSync.incoming["reconciled"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.outgoing["uploaded"].testGetValue())
            XCTAssertNil(GleanMetrics.HistorySync.outgoing["failed_to_upload"].testGetValue())
            XCTAssertNil(GleanMetrics.CreditcardsSync.outgoingBatches.testGetValue())
        }

        func submitTabsPing(_: NoReasonCodes?) {
            globalSyncUuid = GleanMetrics.Sync.syncUuid.testGetValue("tabs-sync")!
            XCTAssertEqual("abc123", GleanMetrics.TabsSync.uid.testGetValue())

            XCTAssertNotNil(GleanMetrics.TabsSync.startedAt.testGetValue())
            XCTAssertNotNil(GleanMetrics.TabsSync.finishedAt.testGetValue())
            XCTAssertEqual(now + 10, Int64(GleanMetrics.TabsSync.startedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)
            XCTAssertEqual(now + 16, Int64(GleanMetrics.TabsSync.finishedAt.testGetValue()!.timeIntervalSince1970) / BaseGleanSyncPing.MILLIS_PER_SEC)

            XCTAssertNil(GleanMetrics.TabsSync.incoming["applied"].testGetValue())
            XCTAssertNil(GleanMetrics.TabsSync.incoming["failed_to_apply"].testGetValue())
            XCTAssertNil(GleanMetrics.TabsSync.incoming["reconciled"].testGetValue())
            XCTAssertEqual(8, GleanMetrics.TabsSync.outgoing["uploaded"].testGetValue())
            XCTAssertEqual(2, GleanMetrics.TabsSync.outgoing["failed_to_upload"].testGetValue())
        }

        try! processSyncTelemetry(syncTelemetry: syncTelemetry,
                                  submitGlobalPing: submitGlobalPing,
                                  submitCreditCardsPing: submitCreditCardsPing,
                                  submitTabsPing: submitTabsPing)
    }
}
