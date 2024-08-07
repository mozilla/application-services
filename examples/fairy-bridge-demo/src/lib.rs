pub use fairy_bridge;

use fairy_bridge::{headers, Request, Response};
use url::Url;

fn make_request() -> Request {
    let url = Url::parse("http://httpbin.org/anything").unwrap();
    Request::get(url)
        .header(headers::USER_AGENT, "fairy-bridge-demo")
        .unwrap()
        .header("X-Foo", "bar")
        .unwrap()
}

#[derive(serde::Serialize)]
struct TestPostData {
    guid: String,
    foo: String,
}

fn make_post_request() -> Request {
    let url = Url::parse("http://httpbin.org/anything").unwrap();
    Request::post(url)
        .header(headers::USER_AGENT, "fairy-bridge-demo")
        .unwrap()
        .header("X-Foo", "bar")
        .unwrap()
        .json(&TestPostData {
            guid: "abcdef1234".to_owned(),
            foo: "Bar".to_owned(),
        })
        .unwrap()
}

#[uniffi::export]
async fn run_demo_async() {
    println!("GET http://httpbin.org/anything (async)");
    let response = make_request().send().await;
    print_response(response);
}

#[uniffi::export]
fn run_demo_sync() {
    println!("GET http://httpbin.org/anything (sync)");
    let response = make_request().send_sync();
    print_response(response);
}

#[uniffi::export]
async fn run_demo_async_post() {
    println!("POST http://httpbin.org/anything (async)");
    let response = make_post_request().send().await;
    print_response(response);
}

#[uniffi::export]
fn run_demo_sync_post() {
    println!("POST http://httpbin.org/anything (sync)");
    let response = make_post_request().send_sync();
    print_response(response);
}

fn print_response(response: fairy_bridge::Result<Response>) {
    match response {
        Ok(response) => {
            println!("got response");
            println!("status: {}", response.status);
            println!("\nHEADERS:");
            for (key, value) in response.headers {
                println!("{}: {}", key, value);
            }
            println!("\nRESPONSE:");
            println!("{}", String::from_utf8(response.body).unwrap());
        }
        Err(e) => {
            println!("error: {e}");
        }
    }
}

uniffi::setup_scaffolding!();
