pub use fairy_bridge;

use fairy_bridge::{headers, Method, Request, RequestSettings, Response};
use url::Url;

fn make_request(settings: RequestSettings) -> Request {
    let url = Url::parse("https://httpbin.org/anything").unwrap();
    Request::new(settings, Method::Get, url)
        .header(headers::USER_AGENT, "fairy-bridge-cli")
        .unwrap()
        .header("X-Foo", "bar")
        .unwrap()
}

#[derive(serde::Serialize)]
struct TestPostData {
    guid: String,
    foo: String,
}

fn make_post_request(settings: RequestSettings) -> Request {
    let url = Url::parse("http://httpbin.org/anything").unwrap();
    Request::new(settings, Method::Post, url)
        .header(headers::USER_AGENT, "fairy-bridge-cli")
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
async fn run_async(settings: RequestSettings) {
    println!("GET https://httpbin.org/anything/ (async)");
    let response = make_request(settings).send().await;
    print_response(response);
}

#[uniffi::export]
fn run_sync(settings: RequestSettings) {
    println!("GET https://httpbin.org/anything/ (sync)");
    let response = make_request(settings).send_sync();
    print_response(response);
}

#[uniffi::export]
async fn run_async_post(settings: RequestSettings) {
    println!("POST http://httpbin.org/anything (async)");
    let response = make_post_request(settings).send().await;
    print_response(response);
}

#[uniffi::export]
fn run_sync_post(settings: RequestSettings) {
    println!("POST http://httpbin.org/anything (sync)");
    let response = make_post_request(settings).send_sync();
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
