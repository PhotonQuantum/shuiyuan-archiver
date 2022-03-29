use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::fs::File;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Local;
use eyre::{Context, ContextCompat, Result};
use fake::Fake;
use fake::faker::name::en::Name;
use futures::future::{BoxFuture, join_all};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use handlebars::Handlebars;
use itertools::Itertools;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use reqwest_middleware::ClientWithMiddleware;
use slint::Weak;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::spawn_blocking;

use crate::future_queue::FutQueue;
use crate::MainWindow;
use crate::models::{Category, Post, RespCategory, RespCooked, RespPost, RespPosts, RespTopic, Topic};

const RESOURCES: &[u8] = include_bytes!("../resources.tar.gz");

pub static HANDLEBARS: Lazy<Handlebars<'_>> = Lazy::new(|| {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(ToString::to_string);
    handlebars.set_strict_mode(true);
    handlebars
        .register_template_file("index", "./templates/index.hbs")
        .unwrap();
    handlebars
});

#[derive(Clone)]
pub struct Archiver {
    client: ClientWithMiddleware,
    topic_id: usize,
    downloaded: Arc<Mutex<HashSet<String>>>,
    to: PathBuf,
    fut_queue: Arc<FutQueue<BoxFuture<'static, Result<()>>>>,
    anonymous: bool,
    fake_name_project: Arc<Mutex<HashMap<String, String>>>,
    ui: Arc<AsyncMutex<Weak<MainWindow>>>,
}

fn extract_resources(to: impl AsRef<Path>) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(RESOURCES)));
    archive.unpack(to)?;
    Ok(())
}

// TODO 抽楼导致楼抓不准，根据 stream 重新分页

impl Archiver {
    pub fn new(
        client: ClientWithMiddleware,
        topic_id: usize,
        to: PathBuf,
        anonymous: bool,
        ui: AsyncMutex<Weak<MainWindow>>,
    ) -> Self {
        Self {
            client,
            topic_id,
            downloaded: Default::default(),
            to,
            fut_queue: Arc::new(FutQueue::new()),
            anonymous,
            fake_name_project: Arc::new(Default::default()),
            ui: Arc::new(ui),
        }
    }
    pub async fn download(self) -> Result<()> {
        fs::create_dir_all(&self.to)?;
        extract_resources(&self.to.join("resources/"))?;

        self.topic(self.topic_id).await?;
        self.fut_queue.finish();
        let mut stream = self.fut_queue.take_stream();
        let mut count = 0;
        while let Some(msg) = stream.next().await {
            msg?;
            count += 1;
            let max_count = self.fut_queue.max_count();
            self.ui.lock().await.upgrade_in_event_loop(move |ui| {
                ui.set_fetch_msg(format!("已下载 {}/{} 个资源文件", count, max_count).into());
            });
        }
        Ok(())
    }
    async fn categories(&self, leaf_id: usize) -> Result<Vec<Category>> {
        let mut res = VecDeque::new();
        let mut current_id = Some(leaf_id);
        while let Some(leaf_id) = current_id {
            let resp: RespCategory = self
                .client
                .get(format!(
                    "https://shuiyuan.sjtu.edu.cn/c/{}/show.json",
                    leaf_id
                ))
                .send()
                .await?
                .json()
                .await?;
            res.push_front(Category {
                name: resp.category.name,
                color: resp.category.color,
            });
            current_id = resp.category.parent_category_id;
        }
        Ok(res.into())
    }
    async fn process_post(&self, post: RespPost) -> Result<Post> {
        static RE_AVATAR: Lazy<Regex> =
            Lazy::new(|| Regex::new(r#"<img .* class="avatar">"#).unwrap());
        let mut cooked = if post.cooked_hidden {
            let resp: RespCooked = self
                .client
                .get(format!(
                    "https://shuiyuan.sjtu.edu.cn/posts/{}/cooked.json",
                    post.id
                ))
                .send()
                .await?
                .json()
                .await
                .wrap_err(format!("unable to reveal {}", post.id))?;
            format!(r#"<p style="color: gray">被折叠的内容</p>{}"#, resp.cooked)
        } else {
            post.cooked
        };
        if self.anonymous {
            cooked = RE_AVATAR.replace_all(&cooked, "").to_string();
        }
        // TODO other actions & hide action if =0
        let likes = post
            .actions_summary
            .into_iter()
            .filter(|a| a.id == 2)
            .find_map(|a| a.count)
            .unwrap_or_default();
        post.retorts.iter().for_each(|r| {
            self.download_asset(
                format!(
                    "https://shuiyuan.sjtu.edu.cn/images/emoji/google/{}.png",
                    r.emoji
                ),
                self.to.join("resources").join(format!("{}.png", r.emoji)),
            );
        });
        let avatar_url = post.avatar_template.replace("{size}", "40");
        let avatar_filename = avatar_url.split('/').last().unwrap();
        if !self.anonymous {
            self.download_asset(
                format!("https://shuiyuan.sjtu.edu.cn{}", avatar_url),
                self.to.join("resources").join(avatar_filename),
            );
        }
        let cooked = self.prepare_cooked(cooked);
        let (name, username, avatar, cooked) = if self.anonymous {
            let mut cooked = cooked;
            let mut fake_name_project = self.fake_name_project.lock();
            fake_name_project.iter().for_each(|(k, v)| {
                cooked = cooked.replace(k, v);
            });
            let fake_name = fake_name_project
                .entry(post.username)
                .or_insert_with(|| Name().fake())
                .clone();
            ("".to_string(), fake_name, None, cooked)
        } else {
            (
                post.name,
                post.username,
                Some(format!("resources/{}", avatar_filename)),
                cooked,
            )
        };
        Ok(Post {
            name,
            number: post.post_number,
            username,
            created_at: post.created_at.to_string(),
            created_at_display: post.created_at.with_timezone(&Local).format("%Y年%m月%d日 %H:%M").to_string(),
            content: cooked,
            likes,
            reply_to: post.reply_to_post_number,
            emojis: post
                .retorts
                .into_iter()
                .map(|r| (r.emoji, r.usernames.len()))
                .collect(),
            avatar,
        })
    }
    fn prepare_cooked(&self, mut content: String) -> String {
        let asset_urls: Vec<_> = extract_asset_url(&content)
            .into_iter()
            .map(|s| {
                (
                    s.clone(),
                    format!("resources/{}", s.split('/').last().unwrap()),
                )
            })
            .collect();
        for (url, name) in &asset_urls {
            content = content.replace(url, name);
        }

        for (url, name) in asset_urls {
            self.download_asset(url, self.to.join(name));
        }
        content
    }
    //noinspection RsTypeCheck
    async fn topic(&self, topic_id: usize) -> Result<()> {
        let resp: RespTopic = self
            .client
            .get(format!("https://shuiyuan.sjtu.edu.cn/t/{}.json", topic_id))
            .send()
            .await?
            .json()
            .await?;
        let base_topic = Topic {
            id: topic_id,
            title: resp.title,
            categories: self.categories(resp.category_id).await?,
            tags: resp.tags,
            posts: vec![],
            prev_page: None,
            next_page: None,
        };

        let posts_count = resp.posts_count;
        let page_size = resp.post_stream.posts.len();
        let pages = (posts_count as f64 / page_size as f64).ceil() as usize;
        let mut futs: FuturesUnordered<_> = resp.post_stream.stream.wrap_err("Missing `stream` field in `post_stream`")?.iter().enumerate()
            .group_by(|(i, _)| i / page_size + 1)    // page
            .into_iter()
            .map(|(page, group)| {
                let post_ids = group.map(|(_, id)| id).copied().collect();
                let this = self.clone();
                let base_topic = base_topic.clone();
                let last_page = page == pages;
                async move { this.posts(base_topic, page, post_ids, last_page).await }
            })
            .collect();
        let mut count = 0;
        while let Some(res) = futs.next().await {
            res?;
            count += 1;
            self.ui.lock().await.upgrade_in_event_loop(move |ui| {
                ui.set_fetch_msg(format!("已获取 {}/{} 页", count, pages).into());
            });
        }
        Ok(())
    }
    async fn posts(
        &self,
        topic: Topic,
        /* base-1 */ page: usize,
        post_ids: Vec<usize>,
        last_page: bool,
    ) -> Result<()> {
        let resp: RespPosts = self
            .client
            .get(format!(
                "https://shuiyuan.sjtu.edu.cn/t/{}/posts.json",
                self.topic_id
            ))
            .query(&post_ids.into_iter().map(|i| ("post_ids[]", i)).collect_vec())
            .send()
            .await?
            .json()
            .await?;
        let processed = join_all(
            resp.post_stream
                .posts
                .into_iter()
                .map(|p| self.process_post(p)),
        )
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let params = Topic {
            posts: processed,
            prev_page: if page > 1 { Some(page - 1) } else { None },
            next_page: if last_page { None } else { Some(page + 1) },
            ..topic
        };
        let output = File::create(self.to.join(format!("{}.html", page)))?;
        Ok(HANDLEBARS.render_to_write("index", &params, output)?)
    }
    fn download_asset(&self, from: String, to: PathBuf) {
        if !self.downloaded.lock().insert(from.clone()) {
            return;
        }
        let client = self.client.clone();
        self.fut_queue.add_future(Box::pin(async move {
            let resp = client.get(from).send().await?.bytes().await?;
            let mut to = File::create(to)?;
            spawn_blocking(move || to.write_all(&resp)).await??;
            Ok(())
        }));
    }
}

fn extract_asset_url(content: &str) -> Vec<String> {
    static IMAGE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r#"https?://shuiyuan.sjtu.edu.cn[^)'",]+.(?:jpg|jpeg|gif|png)"#).unwrap()
    });
    static VIDEO_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"/uploads[^)'",\\]+.mp4"#).unwrap());
    IMAGE_RE
        .captures_iter(content)
        .map(|cap| cap[0].to_string())
        .chain(
            VIDEO_RE
                .captures_iter(content)
                .map(|cap| format!("https://shuiyuan.sjtu.edu.cn{}", cap[0].to_string())),
        )
        .collect()
}
