/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import Foundation

open class LoginRecord {
    /// The guid of this record. When inserting records, you should set this
    /// to the empty string. If you provide a non-empty one to `add`, and it
    /// collides with an existing record, a `LoginsStoreError.DuplicateGuid`
    /// will be emitted.
    public var id: String
    
    /// This record's hostname. Required. Attempting to insert
    /// or update a record to have a blank hostname, will result in a
    /// `LoginsStoreError.InvalidLogin`.
    public var hostname: String
    
    /// This record's password. Required. Attempting to insert
    /// or update a record to have a blank password, will result in a
    /// `LoginsStoreError.InvalidLogin`.
    public var password: String
    
    /// This record's username, if any.
    public var username: String? = nil
    
    /// The challenge string for HTTP Basic authentication.
    ///
    /// Exactly one of `httpRealm` or `formSubmitURL` is allowed to be present,
    /// and attempting to insert or update a record to have both or neither will
    /// result in an `LoginsStoreError.InvalidLogin`.
    public var httpRealm: String? = nil
    
    /// The submission URL for the form where this login may be entered.
    ///
    /// As mentioned above, exactly one of `httpRealm` or `formSubmitURL` is allowed
    /// to be present, and attempting to insert or update a record to have
    /// both or neither will result in an `LoginsStoreError.InvalidLogin`.
    public var formSubmitURL: String? = nil
    
    /// A lower bound on the number of times this record has been "used".
    ///
    /// A use is recorded (and `timeLastUsed` is updated accordingly) in
    /// the following scenarios:
    ///
    /// - Newly inserted records have 1 use.
    /// - Updating a record locally (that is, updates that occur from a
    ///   sync do not count here) increments the use count.
    /// - Calling `touch` on the corresponding id.
    ///
    /// This is ignored by `add` and `update`.
    public var timesUsed: Int = 0
    
    /// An upper bound on the time of creation in milliseconds from the unix epoch.
    ///
    /// This is ignored by `add` and `update`.
    public var timeCreated: Int64 = 0
    
    /// A lower bound on the time of last use in milliseconds from the unix epoch.
    ///
    /// This is ignored by `add` and `update`.
    public var timeLastUsed: Int64 = 0
    
    /// A lower bound on the time of last use in milliseconds from the unix epoch.
    ///
    /// This is ignored by `add` and `update`.
    public var timePasswordChanged: Int64 = 0

    /// HTML field name of the username, if known.
    public var usernameField: String? = nil

    /// HTML field name of the password, if known.
    public var passwordField: String? = nil
    
    
    open func toJSONDict() -> [String: Any] {
        var dict: [String: Any] = [
            "id": self.id,
            "password": self.password,
            "hostname": self.hostname,

            "timesUsed": self.timesUsed,
            "timeCreated": self.timeCreated,
            "timeLastUsed": self.timeLastUsed,
            "timePasswordChanged": self.timePasswordChanged,
        ]
        
        if let username = self.username {
            dict["username"] = username
        }

        if let httpRealm = self.httpRealm {
            dict["httpRealm"] = httpRealm
        }

        if let formSubmitURL = self.formSubmitURL {
            dict["formSubmitURL"] = formSubmitURL
        }
        
        if let passwordField = self.passwordField {
            dict["passwordField"] = passwordField
        }

        if let usernameField = self.usernameField {
            dict["usernameField"] = usernameField
        }
        return dict
    }
    
    open func toJSON() throws -> String {
        // We need a String to pass back to rust.
        let data: Data = try JSONSerialization.data(withJSONObject: self.toJSONDict())
        return String(data: data, encoding: String.Encoding.utf8)!
    }
    
    // TODO: handle errors in these... (they shouldn't ever happen
    // outside of bugs since we write the json in rust, but still)

    public convenience init(fromJSONDict dict: [String: Any]) {
        self.init(
            id: dict["id"] as? String ?? "",
            password: dict["password"] as? String ?? "",
            hostname: dict["hostname"] as? String ?? "",

            username: dict["username"] as? String ?? "",

            formSubmitURL: dict["formSubmitURL"] as? String ?? "",
            httpRealm: dict["httpRealm"] as? String ?? "",

            timesUsed: (dict["timesUsed"] as? Int) ?? 0,
            timeLastUsed: (dict["timeLastUsed"] as? Int64) ?? 0,
            timeCreated: (dict["timeCreated"] as? Int64) ?? 0,
            timePasswordChanged: (dict["timePasswordChanged"] as? Int64) ?? 0,

            usernameField: dict["usernameField"] as? String ?? "",
            passwordField: dict["passwordField"] as? String ?? ""
        )
    }



    init(id: String,
        password: String,
        hostname: String,
        username: String,
        formSubmitURL: String,
        httpRealm: String,
        timesUsed: Int,
        timeLastUsed: Int64,
        timeCreated: Int64,
        timePasswordChanged: Int64,
        usernameField: String,
        passwordField: String) {
            self.id = id
            self.password = password
            self.hostname = hostname
            self.username = username
            self.formSubmitURL = formSubmitURL
            self.httpRealm = httpRealm
            self.timesUsed = timesUsed
            self.timeLastUsed = timeLastUsed
            self.timeCreated = timeCreated
            self.timePasswordChanged = timePasswordChanged
            self.usernameField = usernameField
            self.passwordField = passwordField
    }


    public convenience init(fromJSONString json: String) throws {
        let dict = try JSONSerialization.jsonObject(with: json.data(using: .utf8)!, options: []) as? [String: Any] ?? [String: Any]()
        self.init(fromJSONDict: dict)
    }

    public static func fromJSONArray(_ jsonArray: String) throws -> [LoginRecord] {
        if  let arr = try JSONSerialization.jsonObject(with: jsonArray.data(using: .utf8)!, options: []) as? [[String: Any]] {
            return arr.map { (dict) -> LoginRecord in
                return LoginRecord(fromJSONDict: dict)
            }
        }
        return [LoginRecord]()
    }
}
