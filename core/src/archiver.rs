//! Well this file is really a mess. Good luck if you try to modify it.

use std::fs;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use futures::stream::FuturesOrdered;
use futures::TryStreamExt;
use once_cell::sync::Lazy;
use regex::Regex;
use sanitize_filename::sanitize;
use tokio::sync::Barrier;
use tokio::sync::mpsc::Sender;

use crate::archiver::download_manager::DownloadManager;
use crate::archiver::template::HANDLEBARS;
use crate::client::{Client, MAX_CONN, MAX_THROTTLE_WEIGHT, RequestBuilderExt};
use crate::error::{Error, Result};
use crate::models::{Params, Post, RespPost, RespPosts, Topic, TopicMeta};
use crate::preloaded_store::PreloadedStore;

mod anonymous;
mod download_manager;
mod fetchers;
mod template;
mod utils;

const FETCH_PAGE_SIZE: usize = 400;
const EXPORT_PAGE_SIZE: usize = 20;

/// Download events.
#[derive(Debug, Copy, Clone)]
pub enum DownloadEvent {
    /// Fetching topic metadata.
    FetchingMeta,
    /// Total post chunks to download. It's determined once metadata is fetched.
    PostChunksTotal(usize),
    /// A post chunk is downloaded.
    PostChunksDownloadedInc,
    /// A new resource has been discovered. Total count of resources to download is not known
    /// because of incremental fetching.
    ResourceTotalInc,
    /// A resource is downloaded.
    ResourceDownloadedInc,
}

/// Archive given topic into directory.
///
/// # Arguments
///
/// * `topic_id` - The topic id to archive.
/// * `save_to_base` - The base directory to save the archive to.
/// * `anonymous` - Whether to anonymize usernames.
/// * `reporter` - The sender to send download events to.
///
/// # Errors
///
/// There are many possible errors. See the `Error` enum for details.
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
    let topic_meta = fetchers::fetch_topic_meta(client, topic_id).await?;

    let filename = sanitize(format!(
        "水源_{}_{}",
        &topic_meta.title,
        utils::get_current_time()
    ));
    let save_to = save_to_base.join(filename);
    fs::create_dir_all(save_to.join("resources"))?;
    template::extract_resources(save_to.join("resources"))?;

    // 2. Fetch all posts and download assets.
    let download_manager = DownloadManager::new(client.clone(), save_to.clone(), reporter.clone());
    let mut posts = archive_resp_posts(
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
        let fake_name_map = anonymous::collect_anonymous_names(&posts);
        for post in &mut posts {
            post.name = String::new();
            post.username = fake_name_map
                .get(&post.username)
                .expect("collected")
                .clone();
            post.avatar = None;
            post.content = anonymous::mask_username_in_cooked(&fake_name_map, post.content.clone());
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
    let total_pages = utils::ceil_div(post_count, EXPORT_PAGE_SIZE);
    let last_page = page == total_pages;
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
        format!("{page}.html")
    };
    let output = File::create(save_to.join(filename))?;
    Ok(HANDLEBARS.render_to_write("index", &params, output)?)
}

async fn archive_resp_posts(
    client: &Client,
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    anonymous: bool,
    topic_meta: &TopicMeta,
    reporter: Sender<DownloadEvent>,
) -> Result<Vec<Post>> {
    let topic_id = topic_meta.id;
    let posts_total = topic_meta.post_ids.len();
    let chunks_total = utils::ceil_div(posts_total, FETCH_PAGE_SIZE);
    reporter
        .send(DownloadEvent::PostChunksTotal(chunks_total))
        .await?;

    let barrier = Arc::new(Barrier::new(chunks_total));
    let futs: FuturesOrdered<_> = topic_meta
        .post_ids
        .chunks(FETCH_PAGE_SIZE)
        .map(move |post_ids| {
            let reporter = reporter.clone();
            let barrier = barrier.clone();

            let url = format!("https://shuiyuan.sjtu.edu.cn/t/{topic_id}/posts.json");
            let query: Vec<_> = post_ids.iter().map(|i| ("post_ids[]", i)).collect();
            let req = client
                .get(url)
                .query(&query)
                .with_conn_weight(MAX_CONN as u32)
                .with_throttle_weight(MAX_THROTTLE_WEIGHT);
            async move {
                let resp: RespPosts = client.send_json(req).await?;

                reporter
                    .send(DownloadEvent::PostChunksDownloadedInc)
                    .await?;
                // Continue only after all posts ids are fetched
                barrier.wait().await;

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
                Ok::<_, Error>(posts)
            }
        })
        .collect();

    let nested: Vec<Vec<Post>> = futs.try_collect().await?;
    Ok(nested.into_iter().flatten().collect())
}

async fn process_resp_post(
    client: &Client,
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    anonymous: bool,
    resp_post: RespPost,
) -> Result<Post> {
    static RE_AVATAR: Lazy<Regex> = Lazy::new(|| Regex::new(r#"<img .* class="avatar">"#).unwrap());

    let resp_post = fetchers::fetch_special_post(client, resp_post).await?;
    let cooked = fetchers::fetch_assets_of_content(download_manager, &resp_post.cooked).await?;
    let (cooked, avatar) = if anonymous {
        (RE_AVATAR.replace_all(&cooked, "").to_string(), None)
    } else {
        (
            cooked,
            Some(fetchers::fetch_avatar(download_manager, &resp_post).await?),
        )
    };
    let likes = likes_of_resp_post(&resp_post);
    let futs: FuturesOrdered<_> = resp_post
        .retorts
        .into_iter()
        .map(|r| fetchers::fetch_emoji_from_retort(download_manager, preloaded_store, r))
        .collect();
    let emojis = futs.try_collect().await?;

    Ok(Post {
        name: resp_post.name,
        number: resp_post.post_number,
        username: resp_post.username,
        created_at: resp_post.created_at.to_string(),
        created_at_display: utils::datetime_to_display(&resp_post.created_at),
        content: cooked,
        likes,
        reply_to: resp_post.reply_to_post_number,
        emojis,
        avatar,
    })
}

fn likes_of_resp_post(resp_post: &RespPost) -> usize {
    resp_post
        .actions_summary
        .iter()
        .filter(|a| a.id == 2)
        .find_map(|a| a.count)
        .unwrap_or_default()
}
