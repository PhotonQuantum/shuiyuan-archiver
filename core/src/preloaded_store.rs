use std::collections::HashMap;

use lol_html::{element, HtmlRewriter, RewriteStrSettings};
use reqwest_middleware::ClientWithMiddleware;
use serde::de::{DeserializeOwned, Error};
use serde::{Deserialize, Deserializer};

use crate::error::Result;

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
    pub async fn from_client(client: &ClientWithMiddleware) -> Result<Self> {
        let body = client
            .get("https://shuiyuan.sjtu.edu.cn")
            .send()
            .await?
            .text()
            .await?;
        let mut preloaded = None;
        let rule = element!("#data-preloaded", |el| {
            if preloaded
                .replace(el.get_attribute("data-preloaded").expect("data-preloaded"))
                .is_some()
            {
                panic!("multiple #data-preloaded")
            }
            Ok(())
        });
        let _ = HtmlRewriter::new(
            RewriteStrSettings {
                element_content_handlers: vec![rule],
                ..RewriteStrSettings::default()
            }
            .into(),
            |_: &[u8]| (),
        )
        .write(body.as_bytes());

        let unescaped =
            htmlescape::decode_html(&preloaded.expect("#data-preloaded")).expect("unescaped");

        Ok(serde_json::from_str(&unescaped)?)
    }
    pub fn custom_emoji(&self, name: &str) -> Option<&str> {
        self.custom_emoji.get(name).map(String::as_str)
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
