/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation
import os.log

internal typealias ApiHandle = UInt64
internal typealias ConnHandle = UInt64

/// This is something like a places connection manager. It primarially exists to
/// ensure that only a single write connection is active at once.
///
/// If it helps, you can think of this as something like a connection pool
/// (although it does not actually perform any pooling).
class PlacesApi {
    private let handle: ApiHandle
    private let writeConn: PlacesWriteConn
    fileprivate let queue = DispatchQueue(label: "com.mozilla.places.api")

    /// Initialize a PlacesApi
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

}

class PlacesReadConn {
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
            throw PlacesError.ConnUseAfterApiClosed
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


}

class PlacesWriteConn : PlacesReadConn {

}

