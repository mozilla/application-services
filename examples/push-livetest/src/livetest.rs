/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use push::{BridgeType, PushManager};

/** Perform a "Live" test against a locally configured push server
 *
 * See https://autopush.readthedocs.io/en/latest/testing.html on
 * setting up a local push server. This will also create a local
 * test database under "/tmp". This database should be deleted before
 * you re-run this test.
 *
 * NOTE: if you wish to do a "live" test inside of the kotlin layer,
 * See `PushTest.kt` and look for "LIVETEST".
 */

fn dummy_uuid() -> String {
    // Use easily findable "test" UUIDs
    "deadbeef-ab-dc-ef-abcdef".to_string()
}

fn test_live_server() {
    let tempdir = tempfile::tempdir().unwrap();
    viaduct_reqwest::use_reqwest_backend();

    let pm = PushManager::new(
        "fir-bridgetest".to_string(),
        "localhost:8082".to_string(),
        "http".to_string(),
        BridgeType::Fcm,
        "".to_string(),
        tempdir.path().join("test.db").to_string_lossy().to_string(),
    )
    .unwrap();
    let channel1 = dummy_uuid();
    let channel2 = dummy_uuid();

    pm.update("new-token").unwrap();

    println!("Channels: [{}, {}]", channel1, channel2);

    println!("\n == Subscribing channels");
    let sub1 = pm
        .subscribe(&channel1, "", &None)
        .expect("subscribe failed");

    println!("## Subscription 1: {:?}", sub1);
    println!("## Info: {:?}", pm.dispatch_info_for_chid(&channel1));
    let sub2 = pm.subscribe(&channel2, "", &None).unwrap();
    println!("## Subscription 2: {:?}", sub2);

    println!("\n == Unsubscribing single channel");
    pm.unsubscribe(&channel1).expect("chid unsub failed");

    // the list of known channels should come from whatever is
    // holding the index of channels to recipient applications.
    println!("Verify: {:?}", pm.verify_connection().unwrap());

    // Unsubscribe all channels.
    pm.unsubscribe_all().unwrap();

    println!("Done");
}

fn main() {
    test_live_server()
}
