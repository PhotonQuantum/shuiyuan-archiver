use std::collections::HashMap;

use reqwest_middleware::ClientWithMiddleware;
use scraper::Selector;
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer};

use crate::error::ErrorExt;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreloadedStore {
    #[serde(deserialize_with = "de_from_emojis")]
    custom_emoji: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct Emoji {
    name: String,
    url: String,
}

impl PreloadedStore {
    pub async fn from_client(client: &ClientWithMiddleware) -> super::Result<Self> {
        let selector = Selector::parse("#data-preloaded").unwrap();
        let body = client
            .get("https://shuiyuan.sjtu.edu.cn")
            .send()
            .await?
            .text()
            .await?;
        let document = scraper::Html::parse_document(&body);
        let preloaded = document
            .select(&selector)
            .next()
            .wrap_err("Missing #data-preloaded element.")?
            .value()
            .attr("data-preloaded")
            .wrap_err("Missing data-preloaded attribute.")?;
        Ok(serde_json::from_str(&preloaded)?)
    }
    pub fn custom_emoji(&self, name: &str) -> Option<&str> {
        self.custom_emoji.get(name).map(|s| s.as_str())
    }
}

fn de_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let s = String::deserialize(deserializer)?;
    let mut json_de = serde_json::Deserializer::from_str(&s);
    T::deserialize(&mut json_de).map_err(Error::custom)
}

fn de_from_emojis<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let emojis: Vec<Emoji> = de_from_str(deserializer)?;
    Ok(emojis.into_iter().map(|e| (e.name, e.url)).collect())
}
