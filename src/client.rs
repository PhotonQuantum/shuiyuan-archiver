use std::ops::Deref;
use std::time::UNIX_EPOCH;

use eyre::{bail, ContextCompat, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::{Client, ClientBuilder, Url};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::multipart::{Form, Part};
use serde_json::Value;

const LOGIN_URL: &str = "https://i.sjtu.edu.cn/jaccountlogin";
const LOGIN_POST_URL: &str = "https://jaccount.sjtu.edu.cn/jaccount/ulogin";
const CAPTCHA_URL: &str = "https://jaccount.sjtu.edu.cn/jaccount/captcha";

#[derive(Clone)]
pub struct SJTUClient {
    client: Client,
}

impl Deref for SJTUClient {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl SJTUClient {
    pub async fn new(username: &str, password: &str) -> Result<Self> {
        static RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"uuid": '(.*)'"#).expect("invalid regex"));
        let mut headers = HeaderMap::new();
        headers.insert("Accept", HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,image/apng,*/*;q=0.8"));
        headers.insert("Accept-Language", HeaderValue::from_static("zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7,zh-TW;q=0.6"));

        let client = ClientBuilder::new()
            .default_headers(headers)
            .user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:71.0) Gecko/20100101 Firefox/71.0")
            .cookie_store(true)
            .build()?;

        let resp = client.get(LOGIN_URL).send().await?;
        let url = resp.url().clone();
        let login_params = url.query();

        let uuid = RE
            .captures(resp.text().await?.as_str())
            .wrap_err("Missing UUID on login page")?
            .get(1)
            .wrap_err("Missing UUID on login page")?
            .as_str()
            .to_string();

        let captcha_img = client
            .get(CAPTCHA_URL)
            .query(&[("uuid", &uuid)])
            .query(&[("t", UNIX_EPOCH.elapsed().unwrap().as_millis())])
            .send()
            .await?
            .bytes()
            .await?;
        let part = Part::bytes(captcha_img.to_vec())
            .file_name("captcha.jpg")
            .mime_str("image/jpeg")?;
        let form = Form::new().part("image", part);
        let resp: Value = client
            .post("https://jcss.lightquantum.me")
            .multipart(form)
            .send()
            .await?
            .json()
            .await?;
        let solution = resp
            .pointer("/data/prediction")
            .wrap_err("Failed to solve captcha")?
            .as_str()
            .expect("not a string value")
            .to_string();

        let mut login_req = Url::parse(LOGIN_POST_URL).unwrap();
        login_req.set_query(login_params);

        let resp = client
            .post(login_req)
            .query(&[
                ("v", ""),
                ("uuid", &uuid),
                ("user", username),
                ("pass", password),
                ("captcha", &solution),
            ])
            .send()
            .await?;

        if resp.url().query().unwrap_or_default().contains("err=") {
            bail!("Login failed");
        }

        Ok(Self { client })
    }
}
