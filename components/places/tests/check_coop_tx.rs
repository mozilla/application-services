/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! This test demonstrates how our "cooperative transactions" work and
//! smoketests them. It's slow so it's ignored by default (and only run in CI),
//! but you can run it with `cargo test -p places -- --ignored`.

use places::api::places_api::ConnectionType;
use places::PlacesDb;
use rusqlite::named_params;
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex};
use std::thread;

fn update(t: &PlacesDb, n: u32) -> places::Result<()> {
    use sql_support::ConnExt;
    t.execute_named_cached(
        "INSERT INTO moz_places (guid, url, url_hash) VALUES (:id, :url, hash(:url))",
        named_params! {
            ":id": format!("fake_{:07}", n),
            ":url": format!("http://example.com/{}", n),
        },
    )?;
    Ok(())
}

#[test]
#[ignore] // Ignore by default, but this is run in CI. If you run with `cargo test --ignored` it will be run.
fn check_coop_tx() {
    let _ = init_env_logger_with_thread_id();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");

    let coop_tx_lock = Arc::new(Mutex::new(()));

    let dbmain = PlacesDb::open(&path, ConnectionType::ReadWrite, 0, coop_tx_lock.clone()).unwrap();
    // Ensure we don't autocheckpoint, since we want to test the behavior of slow checkpoints.
    let (tx, rx) = sync_channel(0);

    let child = thread::spawn(move || {
        let db1 = PlacesDb::open(&path, ConnectionType::Sync, 0, coop_tx_lock.clone()).unwrap();
        // assert_eq!(rx.recv().unwrap(), 0);
        let mut t = db1
            .begin_transaction()
            .expect("should get the thread transaction");
        log::info!("inner has tx");
        tx.send(0).unwrap();
        for i in 0..100_000 {
            if (i % 1000) == 0 {
                log::debug!("child thread updating {} / 100k...", i);
            }
            update(&db1, i).unwrap();
            t.maybe_commit().unwrap();
        }
        log::info!("child thread small updates finished, executing full commit");
        t.commit().unwrap();

        // Wait for the main thread to get to the next "phase" of this tests (the phase
        // where everything is slow).
        tx.send(0).unwrap();
        let mut t = db1
            .begin_transaction()
            .expect("should get the thread transaction");
        log::info!("running slow updates");
        // Run some slow updates.
        for i in 100_100..100_105 {
            update(&db1, i).unwrap();
            log::info!("Sleeping for 6 seconds {}", i);
            std::thread::sleep(std::time::Duration::from_secs(6));
            log::info!("Done sleeping, maybe_commit-ing {}", i);
            t.maybe_commit().unwrap();
            log::info!("Child thread (hopefully) committed {}", i);
        }
        t.commit().unwrap();
        log::info!("finished inner thread");
    });

    let _ = rx.recv().unwrap();
    log::info!("inner thread has tx lock, so charging ahead...");
    for i in 100_000..100_020 {
        let tx = dbmain
            .begin_transaction()
            .expect("should get the main transaction");
        update(&dbmain, i).unwrap();
        tx.commit().expect("main thread should commit");
        log::info!("main thread commited {}", i);
    }

    log::info!("completed outer, trying during slow transactions");
    let _ = rx.recv().unwrap();
    for i in 100_020..100_030 {
        let tx = dbmain
            .begin_transaction()
            .expect("should get the main transaction");
        update(&dbmain, i).unwrap();
        log::info!("main thread sleeping for 2s {}", i);
        std::thread::sleep(std::time::Duration::from_secs(2));
        log::info!("main thread committing {}", i);
        tx.commit().expect("main thread should commit");
        log::info!("main thread commited {}", i);
    }

    log::info!("completed outer, waiting for thread to complete.");

    match child.join() {
        Ok(()) => {}
        Err(e) => {
            // Child panicked, this will make sure the backtrace shows up correctly.
            std::panic::resume_unwind(e);
        }
    }
}

/// It's basically impossible to follow the log when we don't know what thread
/// is doing what. This fixes that. It's more complex than would be ideal, but
/// it tries not to give up too much of the default formatting. (Perhaps this
/// was a mistake, but... eh)
fn init_env_logger_with_thread_id() -> std::result::Result<(), log::SetLoggerError> {
    // Need to use a custom formatter to get ThreadId to show up in all logs (which makes this much easier to follow).
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("RUST_LOG", "debug"))
        .format(move |f, record| {
            use std::io::Write as _;
            let mut thread_style = f.style();
            thread_style.set_color(env_logger::fmt::Color::Magenta);

            let thread_id = format!("{:?}", std::thread::current().id());
            let thread_id = if thread_id.starts_with("ThreadId(") && thread_id.ends_with(')') {
                format!("T{}", &thread_id[9..(thread_id.len() - 1)])
            } else {
                // Never happens but could in the future, I guess.
                thread_id
            };

            let level_style = f.default_level_style(record.level());
            let time = f.timestamp();
            writeln!(
                f,
                "{level:<5} {time} on {thread}: {module}: {args}",
                level = level_style.value(record.level()),
                module = record.module_path().unwrap_or("???"),
                time = time,
                thread = thread_style.value(thread_id),
                args = record.args()
            )
        })
        .try_init()
}
