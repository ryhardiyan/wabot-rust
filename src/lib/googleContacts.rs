use crate::{bot_lib::downloader, config::Config};
use anyhow::{Context, Result};
use serde_json::{json, Value};

pub async fn verify_and_save(
    client: &reqwest::Client,
    config: &Config,
    name: &str,
    phone: &str,
) -> Result<Value> {
    let auth = downloader::json(
        client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", config.required("GOOGLE_CLIENT_ID")?),
                ("client_secret", config.required("GOOGLE_CLIENT_SECRET")?),
                ("refresh_token", config.required("GOOGLE_REFRESH_TOKEN")?),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?,
    )
    .await?;
    let token = auth["access_token"]
        .as_str()
        .context("Google OAuth tidak mengembalikan token")?;
    let value = downloader::json(client.post("https://people.googleapis.com/v1/people:createContact")
        .bearer_auth(token).json(&json!({"names":[{"givenName":name}],"phoneNumbers":[{"value":format!("+{phone}"),"type":"mobile"}]})).send().await?).await?;
    anyhow::ensure!(
        value["resourceName"].is_string(),
        "Google Contacts tidak mengonfirmasi resourceName"
    );
    Ok(value)
}
