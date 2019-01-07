/* Any copyright is dedicated to the Public Domain.
http://creativecommons.org/publicdomain/zero/1.0/ */

mod auth;
mod logins;
mod testing;

use crate::auth::{TestAccount, TestClient};
use crate::testing::TestGroup;

macro_rules! cleanup_clients {
    ($($client:ident),+) => {
        crate::auth::cleanup_server(vec![$((&mut $client)),+]).expect("Remote cleanup failed");
        $($client.fully_reset_local_db().expect("Failed to reset client");)+
    };
}

pub fn init_testing() {
    // Enable backtraces.
    std::env::set_var("RUST_BACKTRACE", "1");
    // Turn on trace logging for everything except for a few crates (mostly from
    // our network stack) that happen to be particularly noisy (even on `info`
    // level), which get turned on at the warn level. This can still be
    // overridden with RUST_LOG, however.
    let log_filter = "trace,tokio_threadpool=warn,tokio_reactor=warn,tokio_core=warn,tokio=warn,hyper=warn,want=warn,mio=warn,reqwest=warn";
    env_logger::init_from_env(env_logger::Env::default().filter_or("RUST_LOG", log_filter));
}

pub fn run_test_groups(groups: Vec<TestGroup>) {
    let test_account = TestAccount::new_random().expect("Failed to initialize test account!");
    let mut c0 = TestClient::new(test_account.clone()).expect("new client 0");
    let mut c1 = TestClient::new(test_account.clone()).expect("new client 1");

    log::info!("+ Testing {} groups", groups.len());
    for group in groups {
        log::info!("++ TestGroup begin {}", group.name);
        for (name, test) in group.tests {
            log::info!("+++ Test begin {}::{}", group.name, name);
            test(&mut c0, &mut c1);
            log::info!("+++ Test cleanup {}::{}", group.name, name);
            cleanup_clients!(c0, c1);
            log::info!("+++ Test finish {}::{}", group.name, name);
        }
        log::info!("++ TestGroup end {}", group.name);
    }
    log::info!("+ Test groups finished");
}

pub fn main() {
    println!("### Running sync integration tests ###");
    init_testing();
    run_test_groups(vec![crate::logins::get_test_group()]);
    println!("### Sync integration tests passed!");
}
