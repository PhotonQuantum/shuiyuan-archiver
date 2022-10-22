//! Well this file is really a mess. Good luck if you try to modify it.
use std::collections::hash_map::{DefaultHasher, Entry};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;

use chrono::Local;
use fake::faker::name::en::Name;
use fake::Fake;
use futures::future::{join_all, BoxFuture};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use handlebars::Handlebars;
use html2text::render::text_renderer::TrivialDecorator;
use itertools::Itertools;
use once_cell::sync::Lazy;
use regex::Regex;
use reqwest::header::CONTENT_TYPE;
use reqwest_middleware::ClientWithMiddleware;
use sanitize_filename::sanitize;
use tauri::{Window, Wry};
use tokio::task::spawn_blocking;

use crate::error::ErrorExt;
use crate::future_queue::FutQueue;
use crate::models::{
    Category, Post, RespCategory, RespCooked, RespPost, RespPosts, RespTopic, Topic,
};
use crate::preloaded_store::PreloadedStore;
use crate::shared_promise::{shared_promise_pair, SharedPromise};
use crate::{get_current_time, Result};

const RESOURCES: &[u8] = include_bytes!("../resources.tar.gz");
const TEMPLATE: &str = include_str!("../templates/index.hbs");

pub static HANDLEBARS: Lazy<Handlebars<'_>> = Lazy::new(|| {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(ToString::to_string);
    handlebars.set_strict_mode(true);
    handlebars
        .register_template_string("index", TEMPLATE)
        .unwrap();
    handlebars
});

#[derive(Clone)]
pub struct Archiver {
    client: ClientWithMiddleware,
    topic_id: usize,
    downloaded: Arc<Mutex<HashSet<String>>>,
    downloaded_avatars: Arc<Mutex<HashMap<String, SharedPromise<PathBuf>>>>,
    to_base: PathBuf,
    // !!! This field will be initialized in `Archiver::topic`.
    // Well I admit this field is shit but I'm not going to change it anytime soon cuz I don't have much time.
    to: Arc<Mutex<Option<PathBuf>>>,
    fut_queue: Arc<FutQueue<BoxFuture<'static, Result<()>>>>,
    anonymous: bool,
    fake_name_project: Arc<Mutex<HashMap<String, String>>>,
    window: Window<Wry>,
}

fn extract_resources(to: impl AsRef<Path>) -> Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(RESOURCES)));
    archive.unpack(to)?;
    Ok(())
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

// For toned emoji, see
// https://github.com/discourse/discourse/blob/c85e3e80838d75d8eec132267e2903d729f12aa4/app/models/emoji.rb#L104
fn normalize_emoji(emoji: &str) -> impl Display + '_ {
    static EMOJI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(.+):t([1-6])").unwrap());
    EMOJI_RE.replace_all(emoji.trim_matches(':'), "$1/$2")
}

impl Archiver {
    pub fn new(
        client: ClientWithMiddleware,
        topic_id: usize,
        to_base: PathBuf,
        anonymous: bool,
        window: Window<Wry>,
    ) -> Self {
        Self {
            client,
            topic_id,
            downloaded: Default::default(),
            downloaded_avatars: Default::default(),
            to_base,
            to: Arc::new(Mutex::new(None)),
            fut_queue: Arc::new(FutQueue::new()),
            anonymous,
            fake_name_project: Arc::new(Default::default()),
            window,
        }
    }
    pub async fn download(self) -> Result<()> {
        let preloaded_store = Arc::new(PreloadedStore::from_client(&self.client).await?);

        self.topic(self.topic_id, preloaded_store).await?;

        extract_resources(self.to.lock().unwrap().as_ref().unwrap().join("resources/"))?;

        self.fut_queue.finish();
        let mut stream = self.fut_queue.take_stream();
        let mut count = 0;
        while let Some(msg) = stream.next().await {
            msg?;
            count += 1;
            let max_count = self.fut_queue.max_count();
            self.window
                .emit(
                    "progress-event",
                    format!("正在下载第 {}/{} 个资源文件", count, max_count),
                )
                .expect("failed to emit progress");
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
    async fn process_post(
        &self,
        post: RespPost,
        preloaded_store: Arc<PreloadedStore>,
    ) -> Result<Post> {
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
        let emojis = post
            .retorts
            .into_iter()
            .map(|r| {
                (
                    if let Some(emoji_path) = preloaded_store.custom_emoji(&r.emoji) {
                        // custom emoji
                        let remote_filename = PathBuf::from(emoji_path)
                            .file_name()
                            .unwrap()
                            .to_os_string()
                            .into_string()
                            .unwrap();
                        self.download_asset(
                            emoji_path.to_string(),
                            self.to
                                .lock()
                                .unwrap()
                                .as_ref()
                                .unwrap()
                                .join("resources")
                                .join(&remote_filename),
                        );
                        remote_filename
                    } else {
                        // standard emoji
                        let local_filename = format!("{}.png", r.emoji);
                        let normalized_name = normalize_emoji(&r.emoji);
                        self.download_asset(
                            format!("/images/emoji/google/{}.png", normalized_name),
                            self.to
                                .lock()
                                .unwrap()
                                .as_ref()
                                .unwrap()
                                .join("resources")
                                .join(&local_filename),
                        );
                        local_filename
                    },
                    r.usernames.len(),
                )
            })
            .collect();
        let avatar = if !self.anonymous {
            let avatar_url = post.avatar_template.replace("{size}", "40");
            let avatar_filename = format!(
                "{}_{}",
                calculate_hash(&avatar_url),
                avatar_url.split('/').last().unwrap()
            );
            Some(self.download_avatar(avatar_url, &avatar_filename).await?)
        } else {
            None
        };
        let cooked = self.prepare_cooked(cooked);
        let (name, username, avatar, cooked) = if self.anonymous {
            let mut cooked = cooked;
            let mut fake_name_project = self.fake_name_project.lock().unwrap();
            fake_name_project.iter().for_each(|(k, v)| {
                cooked = cooked.replace(k, v);
            });
            let fake_name = fake_name_project
                .entry(post.username)
                .or_insert_with(|| Name().fake())
                .clone();
            ("".to_string(), fake_name, None, cooked)
        } else {
            (post.name, post.username, avatar, cooked)
        };
        Ok(Post {
            name,
            number: post.post_number,
            username,
            created_at: post.created_at.to_string(),
            created_at_display: post
                .created_at
                .with_timezone(&Local)
                .format("%Y年%m月%d日 %H:%M")
                .to_string(),
            content: cooked,
            likes,
            reply_to: post.reply_to_post_number,
            emojis,
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
            content = content.replace(&format!("https://shuiyuan.sjtu.edu.cn{}", url), name);
            content = content.replace(url, name);
        }

        for (url, name) in asset_urls {
            self.download_asset(url, self.to.lock().unwrap().as_ref().unwrap().join(name));
        }
        content
    }
    //noinspection RsTypeCheck
    async fn topic(&self, topic_id: usize, preloaded_store: Arc<PreloadedStore>) -> Result<()> {
        let resp: RespTopic = self
            .client
            .get(format!("https://shuiyuan.sjtu.edu.cn/t/{}.json", topic_id))
            .send()
            .await?
            .json()
            .await?;
        let description = resp
            .post_stream
            .posts
            .first()
            .map(|post| &*post.cooked)
            .map(|s| {
                html2text::parse(s.as_bytes())
                    .render(40, TrivialDecorator::new())
                    .into_string()
            });
        let base_topic = Topic {
            id: topic_id,
            title: resp.title,
            description,
            categories: self.categories(resp.category_id).await?,
            tags: resp.tags,
            posts: vec![],
            page: None,
            prev_page: None,
            next_page: None,
        };
        let filename = sanitize(format!("水源_{}_{}", &base_topic.title, get_current_time()));
        *self.to.lock().unwrap() = Some(self.to_base.join(filename));
        fs::create_dir_all(self.to.lock().unwrap().as_ref().unwrap().join("resources"))?;

        let posts_count = resp.posts_count;
        let page_size = resp.post_stream.posts.len();
        let pages = (posts_count as f64 / page_size as f64).ceil() as usize;
        let mut futs: FuturesUnordered<_> = resp
            .post_stream
            .stream
            .wrap_err("Missing `stream` field in `post_stream`")?
            .iter()
            .enumerate()
            .group_by(|(i, _)| i / page_size + 1) // page
            .into_iter()
            .map(move |(page, group)| {
                let preloaded_store = preloaded_store.clone();

                let post_ids = group.map(|(_, id)| id).copied().collect();
                let this = self.clone();
                let base_topic = base_topic.clone();
                let last_page = page == pages;
                async move {
                    this.posts(base_topic, page, post_ids, last_page, preloaded_store)
                        .await
                }
            })
            .collect();
        let mut count = 0;
        while let Some(res) = futs.next().await {
            res?;
            count += 1;
            self.window
                .emit(
                    "progress-event",
                    format!("正在获取第 {}/{} 页", count, pages),
                )
                .expect("Fail to emit progress");
        }
        Ok(())
    }
    async fn posts(
        &self,
        topic: Topic,
        /* base-1 */ page: usize,
        post_ids: Vec<usize>,
        last_page: bool,
        preloaded_store: Arc<PreloadedStore>,
    ) -> Result<()> {
        let resp: RespPosts = self
            .client
            .get(format!(
                "https://shuiyuan.sjtu.edu.cn/t/{}/posts.json",
                self.topic_id
            ))
            .query(
                &post_ids
                    .into_iter()
                    .map(|i| ("post_ids[]", i))
                    .collect_vec(),
            )
            .send()
            .await?
            .json()
            .await?;
        let processed = join_all(
            resp.post_stream
                .posts
                .into_iter()
                .map(|p| self.process_post(p, preloaded_store.clone())),
        )
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
        let params = Topic {
            posts: processed,
            page: Some(page),
            prev_page: match page {
                1 => None,
                2 => Some(String::from("index")),
                _ => Some(format!("{}", page - 1)),
            },
            next_page: if last_page { None } else { Some(page + 1) },
            ..topic
        };
        let filename = if page == 1 {
            String::from("index.html")
        } else {
            format!("{}.html", page)
        };
        let output = File::create(self.to.lock().unwrap().as_ref().unwrap().join(filename))?;
        Ok(HANDLEBARS.render_to_write("index", &params, output)?)
    }
    /// Download an avatar.
    ///
    /// Returns new path of the avatar.
    async fn download_avatar(&self, from: String, filename: &str) -> Result<PathBuf> {
        let mut filename = PathBuf::from(filename);
        let mut to = self
            .to
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .join("resources")
            .join(&filename);
        let (swear, promise) = shared_promise_pair();
        let promise = match self.downloaded_avatars.lock().unwrap().entry(from.clone()) {
            Entry::Occupied(promise) => Some(promise.get().clone()),
            Entry::Vacant(e) => {
                e.insert(promise);
                None
            }
        };
        if let Some(promise) = promise {
            return Ok(promise.recv().await);
        }

        let client = self.client.clone();
        let resp = client
            .get(format!("https://shuiyuan.sjtu.edu.cn{}", from))
            .send()
            .await?;
        let content_type = resp.headers().get(CONTENT_TYPE).unwrap();
        if content_type.to_str().unwrap().contains("svg") {
            to.set_extension("svg");
            filename.set_extension("svg");
        }
        let bytes = resp.bytes().await?;

        let mut file = File::create(&to)?;

        spawn_blocking(move || file.write_all(&bytes)).await??;

        let return_path = PathBuf::from("resources").join(filename);
        swear.fulfill(return_path.clone());
        Ok(return_path)
    }
    fn download_asset(&self, from: String, to: PathBuf) {
        if !self.downloaded.lock().unwrap().insert(from.clone()) {
            return;
        }
        let client = self.client.clone();
        self.fut_queue.add_future(Box::pin(async move {
            let resp = client
                .get(format!("https://shuiyuan.sjtu.edu.cn{}", from))
                .send()
                .await?
                .bytes()
                .await?;
            let mut to = File::create(to)?;
            spawn_blocking(move || to.write_all(&resp)).await??;
            Ok(())
        }));
    }
}

#[allow(clippy::to_string_in_format_args)]
fn extract_asset_url(content: &str) -> Vec<String> {
    static IMAGE_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
            r#"https?://shuiyuan.sjtu.edu.cn([^)'",]+.(?:jpg|jpeg|gif|png|JPG|JPEG|GIF|PNG))"#,
        )
        .unwrap()
    });
    static VIDEO_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#"/uploads[^)'",\\]+.(?:mp4|MP4|mov|MOV|avi|AVI)"#).unwrap());
    IMAGE_RE
        .captures_iter(content)
        .map(|cap| cap[1].to_string())
        .chain(
            VIDEO_RE
                .captures_iter(content)
                .map(|cap| cap[0].to_string()),
        )
        .collect()
}
