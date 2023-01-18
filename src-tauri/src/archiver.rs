//! Well this file is really a mess. Good luck if you try to modify it.
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::{DefaultHasher, Entry};
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Local, Utc};
use fake::Fake;
use fake::faker::name::en::Name;
use futures::{stream, TryStreamExt};
use futures::stream::{FuturesOrdered, FuturesUnordered};
use handlebars::{Handlebars, handlebars_helper, html_escape, no_escape};
use html2text::render::text_renderer::TrivialDecorator;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use reqwest::header::CONTENT_TYPE;
use reqwest_middleware::ClientWithMiddleware as Client;
use sanitize_filename::sanitize;
use tokio::sync::mpsc::Sender;
use tokio::task::spawn_blocking;

use crate::{BoxedError, Result};
use crate::action_code::ACTION_CODE_MAP;
use crate::models::{
    Category, Params, Post, RespCategory, RespCooked, RespPost, RespPosts, RespRetort, RespTopic,
    Topic,
};
use crate::preloaded_store::PreloadedStore;
use crate::shared_promise::{shared_promise_pair, SharedPromise};

const RESOURCES: &[u8] = include_bytes!("../resources.tar.gz");
const TEMPLATE: &str = include_str!("../templates/index.hbs");

// Minimum trimmed length for an ascii username to be replaced globally in a post on anonymous mode.
const MIN_ASCII_NAME_LENGTH: usize = 5;
// Minimum trimmed length for a unicode username to be replaced globally in a post on anonymous mode.
const MIN_UNICODE_NAME_LENGTH: usize = 2;

const FETCH_PAGE_SIZE: usize = 400;
const EXPORT_PAGE_SIZE: usize = 20;

static RE_MENTION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<a class="mention" href="/u/.*">@(.*)</a>"#).unwrap());
static RE_QUOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<img .* src=".*" class="avatar"> (.*):</div>"#).unwrap());
static RE_FROM: Lazy<Regex> = Lazy::new(|| Regex::new(r#"来自 (.*)</a>"#).unwrap());

handlebars_helper!(escape: |x: String| html_escape(&x));

pub static HANDLEBARS: Lazy<Handlebars<'_>> = Lazy::new(|| {
    let mut handlebars = Handlebars::new();
    handlebars.register_escape_fn(no_escape);
    handlebars.set_strict_mode(true);
    handlebars.register_helper("escape", Box::new(escape));
    handlebars
        .register_template_string("index", TEMPLATE)
        .unwrap();
    handlebars
});

#[derive(Debug, Copy, Clone)]
pub enum DownloadEvent {
    FetchingMeta,
    PostChunksTotal(usize),
    PostChunksDownloadedInc,
    ResourceTotalInc,
    ResourceDownloadedInc,
}

pub async fn archive(
    client: &Client,
    topic_id: usize,
    save_to_base: &Path,
    anonymous: bool,
    reporter: Sender<DownloadEvent>,
) -> Result<()> {
    // Fetch preload emojis.
    let preloaded_store = PreloadedStore::from_client(client).await?;

    // 1. Fetch topic metadata and create directories.
    reporter.send(DownloadEvent::FetchingMeta).await?;
    let topic_meta = fetch_topic_meta(client, topic_id).await?;

    let filename = sanitize(format!("水源_{}_{}", &topic_meta.title, get_current_time()));
    let save_to = save_to_base.join(filename);
    fs::create_dir_all(save_to.join("resources"))?;
    extract_resources(save_to.join("resources"))?;

    // 2. Fetch all posts and download assets.
    let download_manager = DownloadManager::new(client.clone(), save_to.clone(), reporter.clone());
    let mut posts = fetch_resp_posts(
        client,
        &download_manager,
        &preloaded_store,
        anonymous,
        &topic_meta,
        reporter,
    )
        .await?;

    // 3. If anonymous mode enabled, mask all usernames.
    if anonymous {
        let fake_name_map = collect_anonymous_names(&posts);
        for post in &mut posts {
            post.name = String::new();
            post.username = fake_name_map.get(&post.username).unwrap().clone();
            post.avatar = None;
            post.content = mask_username_in_cooked(&fake_name_map, post.content.clone())
        }
    }

    // 4. Write posts to files.
    posts
        .chunks(EXPORT_PAGE_SIZE)
        .enumerate()
        .try_for_each(move |(page, group)| {
            // TODO remove to_vec
            write_page(topic_meta.clone(), page + 1, group.to_vec(), &save_to)
        })?;

    Ok(())
}

fn write_page(meta: TopicMeta, page: usize, posts: Vec<Post>, save_to: &Path) -> Result<()> {
    let post_count = meta.post_ids.len();
    let total_pages = (post_count as f64 / EXPORT_PAGE_SIZE as f64).ceil() as usize;
    let last_page = page == post_count;
    let topic = Topic {
        id: meta.id,
        title: meta.title,
        description: Some(meta.description), // TODO no some
        categories: meta.categories,
        tags: meta.tags,
        posts,
        page: Some(page), // TODO no Some
        total_pages,
        prev_page: match page {
            1 => None,
            2 => Some(String::from("index")),
            _ => Some(format!("{}", page - 1)),
        },
        next_page: if last_page { None } else { Some(page + 1) },
    };
    let params = Params::from(topic);
    let filename = if page == 1 {
        String::from("index.html")
    } else {
        format!("{}.html", page)
    };
    let output = File::create(save_to.join(filename))?;
    Ok(HANDLEBARS.render_to_write("index", &params, output)?)
}

async fn fetch_avatar(download_manager: &DownloadManager, resp_post: &RespPost) -> Result<PathBuf> {
    let avatar_url = resp_post.avatar_template.replace("{size}", "40");
    let avatar_filename = format!(
        "{}_{}",
        calculate_hash(&avatar_url),
        avatar_url.split('/').last().unwrap()
    );

    download_manager.download_avatar(avatar_url, &avatar_filename).await
}

fn likes_of_resp_post(resp_post: &RespPost) -> usize {
    resp_post
        .actions_summary
        .iter()
        .filter(|a| a.id == 2)
        .find_map(|a| a.count)
        .unwrap_or_default()
}

fn datetime_to_display(datetime: &DateTime<Utc>) -> String {
    datetime
        .with_timezone(&Local)
        .format("%Y年%m月%d日 %H:%M")
        .to_string()
}

async fn fetch_emoji_from_retort(
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    r: RespRetort,
) -> Result<(String, usize)> {
    let filename = if let Some(emoji_path) = preloaded_store.custom_emoji(&r.emoji) {
        let filename = emoji_path.rsplit('/').next().unwrap();
        download_manager
            .download_asset(emoji_path.to_string(), filename)
            .await?;
        filename.to_string()
    } else {
        let filename = format!("{}.png", r.emoji);
        let url = format!("/images/emoji/google/{}.png", normalize_emoji(&r.emoji));
        download_manager.download_asset(url, &filename).await?;
        filename
    };
    let count = r.usernames.len();
    Ok((filename, count))
}

async fn process_resp_post(
    client: &Client,
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    anonymous: bool,
    resp_post: RespPost,
) -> Result<Post> {
    static RE_AVATAR: Lazy<Regex> = Lazy::new(|| Regex::new(r#"<img .* class="avatar">"#).unwrap());

    let resp_post = cook_special_post(client, resp_post).await?;
    let cooked = fetch_assets_by_cooked(download_manager, &resp_post.cooked).await?;
    let (cooked, avatar) = if anonymous {
        (RE_AVATAR.replace_all(&cooked, "").to_string(), None)
    } else {
        (
            cooked,
            Some(fetch_avatar(download_manager, &resp_post).await?),
        )
    };
    let likes = likes_of_resp_post(&resp_post);
    let futs: FuturesOrdered<_> = resp_post
        .retorts
        .into_iter()
        .map(|r| fetch_emoji_from_retort(download_manager, preloaded_store, r))
        .collect();
    let emojis = futs.try_collect().await?;

    Ok(Post {
        name: resp_post.name,
        number: resp_post.post_number,
        username: resp_post.username,
        created_at: resp_post.created_at.to_string(),
        created_at_display: datetime_to_display(&resp_post.created_at),
        content: cooked,
        likes,
        reply_to: resp_post.reply_to_post_number,
        emojis,
        avatar,
    })
}

fn ceil_div(x: usize, y: usize) -> usize {
    x / y + (x % y != 0) as usize
}

async fn fetch_resp_posts(
    client: &Client,
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    anonymous: bool,
    topic_meta: &TopicMeta,
    reporter: Sender<DownloadEvent>,
) -> Result<Vec<Post>> {
    let topic_id = topic_meta.id;
    let posts_total = topic_meta.post_ids.len();
    let chunks_total = ceil_div(posts_total, FETCH_PAGE_SIZE);
    println!("posts_total/fetch_page_size = {}/{} = {}", posts_total, FETCH_PAGE_SIZE, chunks_total);
    reporter.send(DownloadEvent::PostChunksTotal(chunks_total)).await?;

    let futs: FuturesOrdered<_> = topic_meta
        .post_ids
        .chunks(FETCH_PAGE_SIZE)
        .map(move |post_ids| {
            let reporter = reporter.clone();

            let url = format!("https://shuiyuan.sjtu.edu.cn/t/{}/posts.json", topic_id);
            let query: Vec<_> = post_ids.iter().map(|i| ("post_ids[]", i)).collect();
            async move {
                let resp: RespPosts = client.get(url).query(&query).send().await?.json().await?;

                let futs: FuturesOrdered<_> = resp
                    .post_stream
                    .posts
                    .into_iter()
                    .map(|resp_post| {
                        process_resp_post(
                            client,
                            download_manager,
                            preloaded_store,
                            anonymous,
                            resp_post,
                        )
                    })
                    .collect();
                let posts: Vec<Post> = futs.try_collect().await?;

                reporter
                    .send(DownloadEvent::PostChunksDownloadedInc)
                    .await?;
                Ok::<_, BoxedError>(posts)
            }
        })
        .collect();

    let nested: Vec<Vec<Post>> = futs.try_collect().await?;
    Ok(nested.into_iter().flatten().collect())
}

async fn fetch_assets_by_cooked(
    download_manager: &DownloadManager,
    content: &str,
) -> Result<String> {
    let asset_urls: Vec<_> = extract_asset_url(content)
        .into_iter()
        .map(|s| {
            (
                s.clone(),
                s.split('/').last().unwrap().to_string(),
            )
        })
        .collect();

    let mut content = content.to_string();
    for (url, name) in &asset_urls {
        content = content.replace(&format!("https://shuiyuan.sjtu.edu.cn{}", url), &format!("resources/{}", name));
        content = content.replace(url, &format!("resources/{}", name));
    }

    let futs: FuturesUnordered<_> = asset_urls
        .into_iter()
        .map(|(url, name)| async move { download_manager.download_asset(url, &name).await })
        .collect();
    futs.try_collect().await?;

    Ok(content)
}

pub fn collect_anonymous_names<'a>(
    posts: impl IntoIterator<Item=&'a Post> + Clone,
) -> HashMap<String, String> {
    let mut fake_name_map = HashMap::new();
    for post in posts.clone() {
        if !fake_name_map.contains_key(&post.username) {
            let project: String = Name().fake();
            fake_name_map.insert(post.username.clone(), project.clone());
            fake_name_map.insert(post.name.clone(), project);
        }
    }
    for post in posts {
        // Note: we only get username for mention and name for quote here.
        // Theoretically we should fetch the other one too but to avoid network traffic we don't.
        for re in [&RE_MENTION, &RE_QUOTE, &RE_FROM] {
            for cap in re.captures_iter(&post.content) {
                fake_name_map
                    .entry(
                        cap.get(1)
                            .expect("has at least one group")
                            .as_str()
                            .to_string(),
                    )
                    .or_insert_with(|| Name().fake());
            }
        }
    }
    fake_name_map
}

fn mask_username_in_cooked(fake_name_map: &HashMap<String, String>, mut s: String) -> String {
    #[allow(clippy::type_complexity)]
    let re_f: &[(_, fn(&str) -> String)] = &[
        (&RE_MENTION, |fake_name| {
            format!(r#"<a class="mention">@{}</a>"#, fake_name)
        }),
        (&RE_QUOTE, |fake_name| format!(r#" {}:</div>"#, fake_name)),
        (&RE_FROM, |fake_name| format!(r#"来自 {}</a>"#, fake_name)),
    ];
    for (re, f) in re_f {
        s = re
            .replace_all(&s, |caps: &Captures| {
                let name = caps.get(1).expect("has at least one group");
                let fake_name = fake_name_map
                    .get(name.as_str())
                    .expect("should have been collected")
                    .as_str();
                f(fake_name)
            })
            .to_string();
    }

    fake_name_map.iter().fold(s, |s, (name, fake_name)| {
        match (name.is_ascii(), name.trim().len()) {
            (true, l) if l >= MIN_ASCII_NAME_LENGTH => s.replace(name, fake_name),
            (false, l) if l >= MIN_UNICODE_NAME_LENGTH => s.replace(name, fake_name),
            _ => s,
        }
    })
}

struct DownloadManager {
    client: Client,
    downloaded_assets: Mutex<HashSet<String>>,
    downloaded_avatars: Mutex<HashMap<String, SharedPromise<PathBuf>>>,
    save_to: PathBuf,
    reporter: Sender<DownloadEvent>,
}

impl DownloadManager {
    pub fn new(client: Client, save_to: PathBuf, reporter: Sender<DownloadEvent>) -> Self {
        Self {
            client,
            save_to,
            downloaded_assets: Mutex::new(HashSet::new()),
            downloaded_avatars: Mutex::new(HashMap::new()),
            reporter,
        }
    }
}

impl DownloadManager {
    async fn download_asset(&self, from: String, filename: &str) -> Result<()> {
        if !self.downloaded_assets.lock().unwrap().insert(from.clone()) {
            return Ok(());
        }

        self.reporter.send(DownloadEvent::ResourceTotalInc).await?;

        let url = format!("https://shuiyuan.sjtu.edu.cn{}", from);
        let resp = self.client.get(url).send().await?.bytes().await?;

        let save_path = self.save_to.join("resources").join(filename);
        let mut to = File::create(save_path)?;

        spawn_blocking(move || to.write_all(&resp)).await??;

        self.reporter.send(DownloadEvent::ResourceDownloadedInc).await?;
        Ok(())
    }
    async fn download_avatar(&self, from: String, filename: &str) -> Result<PathBuf> {
        let mut filename = PathBuf::from(filename);
        let relative_path = PathBuf::from("resources").join(&filename);
        let mut save_path = self.save_to.join(&relative_path);

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

        self.reporter.send(DownloadEvent::ResourceTotalInc).await?;

        let url = format!("https://shuiyuan.sjtu.edu.cn{}", from);
        let resp = self.client.get(url).send().await?;
        let content_type = resp.headers().get(CONTENT_TYPE).unwrap();
        if content_type.to_str().unwrap().contains("svg") {
            save_path.set_extension("svg");
            filename.set_extension("svg");
        }
        let bytes = resp.bytes().await?;

        let mut file = File::create(&save_path)?;

        spawn_blocking(move || file.write_all(&bytes)).await??;
        swear.fulfill(relative_path.clone());

        self.reporter.send(DownloadEvent::ResourceDownloadedInc).await?;
        Ok(relative_path)
    }
}

#[derive(Clone)]
pub struct TopicMeta {
    pub id: usize,
    pub title: String,
    pub description: String,
    pub categories: Vec<Category>,
    pub tags: Vec<String>,
    pub post_ids: Vec<usize>,
}

/// Fetch topic meta data.
async fn fetch_topic_meta(client: &Client, topic_id: usize) -> Result<TopicMeta> {
    let url = format!("https://shuiyuan.sjtu.edu.cn/t/{}.json", topic_id);
    let resp: RespTopic = client.get(url).send().await?.json().await?;

    let first_post = resp.post_stream.posts.first().expect("at least one post");
    let description = summarize(&first_post.cooked);

    Ok(TopicMeta {
        id: topic_id,
        title: resp.title,
        description,
        categories: categories_from_id(client, resp.category_id).await?,
        tags: resp.tags,
        post_ids: resp.post_stream.stream.expect("exists"),
    })
}

/// Get category names from a leaf category id.
async fn categories_from_id(client: &Client, leaf_id: usize) -> Result<Vec<Category>> {
    stream::try_unfold(leaf_id, |id| async move {
        let url = format!("https://shuiyuan.sjtu.edu.cn/c/{}/show.json", id);
        let resp: RespCategory = client.get(url).send().await?.json().await?;

        let yielded = resp.category.inner;
        let next = resp.category.parent_category_id;
        Ok(next.map(|id| (yielded, id)))
    })
        .try_collect()
        .await
}

/// Reveal hidden posts and convert system messages.
async fn cook_special_post(client: &Client, post: RespPost) -> Result<RespPost> {
    if let Some((_, system_msg)) = post
        .action_code
        .as_ref()
        .and_then(|code| ACTION_CODE_MAP.iter().find(|(c, _)| c == code))
    {
        Ok(RespPost {
            cooked: format!("<p>系统消息：{}</p>", system_msg),
            ..post
        })
    } else if post.cooked_hidden {
        let url = format!("https://shuiyuan.sjtu.edu.cn/posts/{}/cooked.json", post.id);
        let resp: RespCooked = client.get(url).send().await?.json().await?;
        Ok(RespPost {
            cooked: format!(r#"<p style="color: gray">被折叠的内容</p>{}"#, resp.cooked),
            ..post
        })
    } else {
        Ok(post)
    }
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

#[allow(clippy::to_string_in_format_args)]
fn extract_asset_url(content: &str) -> Vec<String> {
    const IMAGE_SUFFIX: &str = "jpg|jpeg|gif|png|JPG|JPEG|GIF|PNG";
    const VIDEO_SUFFIX: &str = "mp4|mov|avi|MP4|MOV|AVI";
    // TODO regex!
    static FULL_URL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(&format!(
            r#"https?://shuiyuan.sjtu.edu.cn([^)'",]+.(?:{IMAGE_SUFFIX}|{VIDEO_SUFFIX}))"#
        ))
            .unwrap()
    });
    static UPLOAD_URL_RE: Lazy<Regex> = Lazy::new(|| {
        Regex::new(&format!(
            r#"/uploads[^)'",\\]+.(?:{IMAGE_SUFFIX}|{VIDEO_SUFFIX})"#
        ))
            .unwrap()
    });
    let full_url_caps = FULL_URL_RE
        .captures_iter(content)
        .map(|cap| cap[1].to_string());
    let upload_url_caps = UPLOAD_URL_RE
        .captures_iter(content)
        .map(|cap| cap[0].to_string());
    full_url_caps.chain(upload_url_caps).collect()
}

fn get_current_time() -> String {
    Local::now().format("%Y-%m-%d_%H:%M:%S").to_string()
}

fn summarize(content: &str) -> String {
    html2text::parse(content.as_bytes())
        .render(120, TrivialDecorator::new())
        .into_string()
}
