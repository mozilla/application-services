use url::Url;
use viaduct::{configure_ohttp_channel, OhttpConfig, Request, Client, GLOBAL_SETTINGS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Initialize viaduct
    {
        let mut settings = GLOBAL_SETTINGS.write();
        settings.addn_allowed_insecure_url = Some(Url::parse("http://34.59.207.77/")?);
    }
    viaduct::init_backend_hyper()?;

    // Step 2: Configure OHTTP
    configure_ohttp_channel("merino".to_string(), OhttpConfig {
        relay_url: "http://34.59.207.77/".to_string(),
        target_host: "localhost".to_string(),
    })?;

    // Step 3: Create request
    let request = Request::post(Url::parse("http://localhost/api/v2/curated-recommendations")?)
        .ohttp_channel("merino")?
        .header("Content-Type", "application/json")?
        .json(&serde_json::json!({"locale": "en-US", "count": 1}));

    // Step 4: Send request
    let client = Client::default();
    let response = client.send_sync(request)?;

    // Step 5: Read response
    println!("Status: {}", response.status);
    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&response.body) {
        if let Some(title) = json.pointer("/data/0/title") {
            println!("Title: {}", title);
        }
    }

    Ok(())
}
