use std::collections::HashMap;
use std::fs;

use std::path::{Path, PathBuf};

use eyre::Result;
use futures::future::try_join_all;

use crate::MainWindow;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::Client;
use slint::Weak;
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

#[derive(Eq, PartialEq)]
enum ResourceType {
    Video,
    Other,
}

pub async fn download(
    client: &Client,
    topic: u64,
    output: &Path,
    ui: AsyncMutex<Weak<MainWindow>>,
) -> Result<()> {
    static IMAGE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"https?://shuiyuan.sjtu.edu.cn[^)'",]+.(?:jpg|jpeg|gif|png)"#).unwrap()
    });
    static VIDEO_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/uploads[^)'",\\]+.mp4"#).unwrap());
    static CSS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/stylesheets.*.css"#).unwrap());
    static JS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/theme-javascripts.*.js"#).unwrap());

    fs::create_dir_all(output.join("resources"))?;

    let mut resource_map = HashMap::new();

    let mut page = 1;
    while let Some(mut src) = fetch_page(client, topic, page).await? {
        ui.lock().await.upgrade_in_event_loop(move |ui| {
            ui.set_fetch_msg(format!("下载第 {} 页中...", page).into())
        });

        let videos = VIDEO_RE.captures_iter(&src);
        let images = IMAGE_RE.captures_iter(&src);
        let css = CSS_RE.captures_iter(&src);
        let js = JS_RE.captures_iter(&src);

        let resources: Vec<_> = images
            .chain(css)
            .chain(js)
            .map(|c| (c, ResourceType::Other))
            .chain(videos.map(|c| (c, ResourceType::Video)))
            .map(|(c, t)| (c.get(0).unwrap().as_str().to_string(), t))
            .collect();

        for (name, ty) in resources {
            let (_, ext) = name.rsplit_once('.').unwrap();
            let new_name = resource_map
                .entry(name.clone())
                .or_insert_with(|| format!("{}.{}", Uuid::new_v4(), ext));

            if ty == ResourceType::Video {
                src = src.replace(
                    &format!("https://shuiyuan.sjtu.edu.cn{}", name),
                    &format!("resources/{}", new_name),
                );
            }
            src = src.replace(&name, &format!("resources/{}", new_name));
        }

        let src = src
            .replace(
                &format!("/t/topic/{}?page={}", topic, page + 1),
                &format!("{}.html", page + 1),
            )
            .replace(
                &format!("/t/topic/{}?page={}", topic, page - 1),
                &format!("{}.html", page - 1),
            )
            .replace(&format!("href=\"/t/topic/{}\"", topic), "href=\"1.html\"");

        fs::write(output.join(format!("{}.html", page)), src)?;

        page += 1;
    }

    ui.lock()
        .await
        .upgrade_in_event_loop(|ui| ui.set_fetch_msg("下载资源中...".into()));
    let _ = try_join_all(
        resource_map
            .into_iter()
            .map(|(src, dest)| fetch_resource(client, src, output.join("resources").join(dest))),
    )
    .await?;
    Ok(())
}

async fn fetch_resource(client: &Client, src: String, dest: PathBuf) -> Result<()> {
    let src = if src.starts_with("http") {
        src
    } else {
        format!("https://shuiyuan.sjtu.edu.cn{}", src)
    };
    let resp = client.get(src).send().await?.bytes().await?;
    fs::write(dest, resp)?;
    Ok(())
}

async fn fetch_page(client: &Client, topic: u64, page: u64) -> Result<Option<String>> {
    let resp = client
        .get(format!("https://shuiyuan.sjtu.edu.cn/t/topic/{}", topic))
        .query(&[("page", page)])
        .query(&[("_escaped_fragment_", true)])
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    Ok(Some(resp.text().await?))
}
