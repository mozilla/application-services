/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/// Given a string persisted as our old GlobalState V1 struct, extract out
/// the global and collection sync guids.
/// Returns Some(global_sync_id, collection_sync_id), or None if the json is
/// invalid, the collection is not listed, or is flagged for reset.
/// XXX - we should probably extract "declined" too.
pub fn extract_v1_sync_ids(
    state: Option<String>,
    collection: &'static str,
) -> Option<(String, String)> {
    let state = match state {
        Some(s) => s,
        None => return None,
    };
    let j: serde_json::Value = match serde_json::from_str(&state) {
        Ok(j) => j,
        Err(_) => return None,
    };
    if Some("V1") != j.get("schema_version").and_then(|v| v.as_str()) {
        return None;
    }

    // See if the collection needs a reset.
    let empty = Vec::<serde_json::Value>::new();
    for change in j
        .get("engine_state_changes")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty)
    {
        if change.as_str() == Some("ResetAll") {
            return None;
        }
        // other resets we care about are objects - `"Reset":name` and
        // `"ResetAllExcept":[name, name]`
        if let Some(change_ob) = change.as_object() {
            if change_ob.get("Reset").and_then(|v| v.as_str()) == Some(collection) {
                // this engine is reset.
                return None;
            }
            if let Some(except_array) = change_ob.get("ResetAllExcept").and_then(|v| v.as_array()) {
                // We have what appears to be a valid list of exceptions to reset.
                // If every one lists an engine that isn't us, we are being reset.
                if except_array
                    .iter()
                    .filter_map(|v| v.as_str())
                    .all(|s| s != collection)
                {
                    return None;
                }
            }
        }
    }

    // Try and find the sync guids in the global payload.
    let global = match j.get("global").and_then(|v| v.as_object()) {
        None => return None,
        Some(v) => v,
    };
    // payload is itself a string holding json - so re-parse.
    let payload: serde_json::Value = match global["payload"]
        .as_str()
        .and_then(|s| serde_json::from_str(s).ok())
    {
        Some(p) => p,
        None => return None,
    };
    let gsid = payload["syncID"].as_str().map(|s| s.to_string());
    let csid = payload["engines"]
        .as_object()
        .and_then(|e| e.get(collection))
        .and_then(|e| e.get("syncID"))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    match (gsid, csid) {
        (Some(g), Some(c)) => Some((g, c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test our destructuring of the old persisted global state.

    fn get_state_with_engine_changes(changes: &str) -> String {
        // This is a copy of the V1 persisted state.
        // Note some things have been omitted or trimmed from what's actually persisted
        // (eg, top-level "config" is removed, "collections" is removed (that's only timestamps)
        // hmac keys have array elts removed, global/payload has engines removed, etc)
        // Note also that all `{` and `}` have been doubled for use in format!(),
        // which we use to patch-in engine_state_changes.
        format!(r#"{{
            "schema_version":"V1",
            "global":{{
                "id":"global",
                "collection":"",
                "payload":"{{\"syncID\":\"qZKAMjhyV6Ti\",\"storageVersion\":5,\"engines\":{{\"addresses\":{{\"version\":1,\"syncID\":\"8M-HfX6dm-pD\"}},\"bookmarks\":{{\"version\":2,\"syncID\":\"AVXtnKkH5OTi\"}}}},\"declined\":[]}}"
            }},
            "keys":{{"timestamp":1548214240.34,"default":{{"enc_key":[36,76],"mac_key":[222,241]}},"collections":{{}}}},
            "engine_state_changes":[
                {}
            ]
        }}"#, changes)
    }

    #[test]
    fn test_extract_state_simple() {
        let s = get_state_with_engine_changes("");
        assert_eq!(
            extract_v1_sync_ids(Some(s.clone()), "addresses"),
            Some(("qZKAMjhyV6Ti".to_string(), "8M-HfX6dm-pD".to_string()))
        );
        assert_eq!(
            extract_v1_sync_ids(Some(s.clone()), "bookmarks"),
            Some(("qZKAMjhyV6Ti".to_string(), "AVXtnKkH5OTi".to_string()))
        );
    }

    #[test]
    fn test_extract_with_engine_reset_all() {
        let s = get_state_with_engine_changes("\"ResetAll\"");
        assert_eq!(extract_v1_sync_ids(Some(s), "addresses"), None);
    }

    #[test]
    fn test_extract_with_engine_reset() {
        let s = get_state_with_engine_changes("{\"Reset\" : \"addresses\"}");
        assert_eq!(extract_v1_sync_ids(Some(s.clone()), "addresses"), None);
        // bookmarks wasn't reset.
        assert_eq!(
            extract_v1_sync_ids(Some(s.clone()), "bookmarks"),
            Some(("qZKAMjhyV6Ti".to_string(), "AVXtnKkH5OTi".to_string()))
        );
    }

    #[test]
    fn test_extract_with_engine_reset_except() {
        let s = get_state_with_engine_changes("{\"ResetAllExcept\" : [\"addresses\"]}");
        // addresses is the exception
        assert_eq!(
            extract_v1_sync_ids(Some(s.clone()), "addresses"),
            Some(("qZKAMjhyV6Ti".to_string(), "8M-HfX6dm-pD".to_string()))
        );
        // bookmarks was reset.
        assert_eq!(extract_v1_sync_ids(Some(s.clone()), "bookmarks"), None);
    }
}
