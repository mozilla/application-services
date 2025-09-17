#[cfg(all(feature = "ohttp", feature = "backend-dev"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use url::Url;
    use viaduct::{configure_ohttp_channel, Client, OhttpConfig, Request, GLOBAL_SETTINGS};

    env_logger::init();

    // Step 1: Initialize viaduct
    {
        let mut settings = GLOBAL_SETTINGS.write();
        // This IP address points towards a sandbox OHTTP Rust gateway, with an adjusted Merino service
        // behind it. If this returns 404, it means the sandbox is down. Please replace it with a working
        // OHTTP relay.
        settings.addn_allowed_insecure_url = Some(Url::parse("http://34.59.207.77/")?);
        println!("Added insecure URL allowlist for relay");
    }

    println!("Initializing viaduct backend...");
    viaduct::init_backend_dev();
    println!("Backend initialized successfully");

    // Step 2: Configure OHTTP
    println!("Configuring OHTTP channel...");
    configure_ohttp_channel(
        "merino".to_string(),
        OhttpConfig {
            relay_url: "http://34.59.207.77/".to_string(),
            target_host: "localhost".to_string(),
        },
    )?;
    println!("OHTTP channel configured");

    // Step 3: Create request
    println!("Creating request...");
    let request = Request::post(Url::parse(
        "http://localhost/api/v2/curated-recommendations",
    )?)
    .ohttp_channel("merino")?
    .header("Content-Type", "application/json")?
    .json(&serde_json::json!({"locale": "en-US", "count": 1}));
    println!(
        "Request created: {} {}",
        request.method.as_str(),
        request.url
    );

    // Step 4: Send request
    println!("Sending request...");
    let client = Client::default();
    let response = client.send_sync(request)?;

    // Step 5: Read response
    println!("Response received!");
    println!("Status: {}", response.status);
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response.body) {
        if let Some(title) = json.pointer("/data/0/title") {
            println!("Title: {}", title);
        }
        println!("Full response: {}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("Response body: {}", String::from_utf8_lossy(&response.body));
    }

    Ok(())
}

#[cfg(not(all(feature = "ohttp", feature = "backend-dev")))]
fn main() {
    println!("This example requires the 'ohttp' and 'backend-dev' features to be enabled.");
    println!("Run with: cargo run --example example_ohttp_flow --features ohttp,backend-dev");
}
