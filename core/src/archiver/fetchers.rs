use std::path::PathBuf;

use futures::stream::FuturesUnordered;
use futures::{stream, TryStreamExt};
use once_cell::sync::Lazy;
use regex::Regex;
use tap::TapFallible;
use tracing::error;

use crate::action_code::ACTION_CODE_MAP;
use crate::archiver::download_manager::DownloadManager;
use crate::archiver::utils;
use crate::client::Client;
use crate::error;
use crate::models::{
    Category, RespCategory, RespCooked, RespPost, RespRetort, RespTopic, TopicMeta,
};
use crate::preloaded_store::PreloadedStore;

const IMAGE_SUFFIXES: [&str; 4] = ["jpg", "jpeg", "gif", "png"];
const VIDEO_SUFFIXES: [&str; 3] = ["mp4", "mov", "avi"];

pub async fn fetch_avatar(
    download_manager: &DownloadManager,
    resp_post: &RespPost,
) -> error::Result<PathBuf> {
    let avatar_url = resp_post.avatar_template.replace("{size}", "40");
    let avatar_filename = format!(
        "{}_{}",
        utils::calculate_hash(&avatar_url),
        avatar_url.split('/').last().unwrap()
    );

    download_manager
        .download_avatar(avatar_url, &avatar_filename)
        .await
        .tap_err(|e| error!(post = resp_post.id, ?e, "Failed to download avatar"))
}

pub async fn fetch_emoji_from_retort(
    download_manager: &DownloadManager,
    preloaded_store: &PreloadedStore,
    r: RespRetort,
) -> error::Result<(String, usize)> {
    let filename = if let Some(emoji_path) = preloaded_store.custom_emoji(&r.emoji) {
        let filename = emoji_path.rsplit('/').next().unwrap();
        download_manager
            .download_asset(emoji_path.to_string(), filename, false)
            .await?;
        filename.to_string()
    } else {
        let filename = format!("{}.png", r.emoji);
        let url = format!(
            "/images/emoji/google/{}.png",
            utils::normalize_emoji(&r.emoji)
        );
        download_manager
            .download_asset(url, &filename, false)
            .await?;
        filename
    };
    let count = r.usernames.len();
    Ok((filename, count))
}

pub async fn fetch_assets_of_content(
    download_manager: &DownloadManager,
    content: &str,
) -> error::Result<String> {
    let asset_urls: Vec<_> = extract_asset_url(content)
        .into_iter()
        .map(|s| (s.clone(), s.split('/').last().unwrap().to_string()))
        .collect();

    let mut content = content.to_string();
    for (url, name) in &asset_urls {
        content = content.replace(
            &format!("https://shuiyuan.sjtu.edu.cn{url}"),
            &format!("resources/{name}"),
        );
        content = content.replace(url, &format!("resources/{name}"));
    }

    let futs: FuturesUnordered<_> = asset_urls
        .into_iter()
        .map(|(url, name)| async move {
            let bypass_limit = VIDEO_SUFFIXES
                .into_iter()
                .all(|ext| !url.to_lowercase().ends_with(ext));
            download_manager
                .download_asset(url, &name, bypass_limit)
                .await
        })
        .collect();
    futs.try_collect().await?;

    Ok(content)
}

/// Fetch topic meta data.
///
/// # Errors
///
/// Returns error if failed to fetch topic meta or failed to fetch category names.
pub async fn fetch_topic_meta(client: &Client, topic_id: u32) -> error::Result<TopicMeta> {
    let url = format!("https://shuiyuan.sjtu.edu.cn/t/{topic_id}.json");
    let resp: RespTopic = client.send_json(client.get(url)).await?;

    let first_post = resp.post_stream.posts.first().expect("at least one post");
    let description = utils::summarize(&first_post.cooked);

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
async fn categories_from_id(client: &Client, leaf_id: usize) -> error::Result<Vec<Category>> {
    stream::try_unfold(leaf_id, |id| async move {
        let url = format!("https://shuiyuan.sjtu.edu.cn/c/{id}/show.json");
        let resp: RespCategory = client.send_json(client.get(url)).await?;

        let yielded = resp.category.inner;
        let next = resp.category.parent_category_id;
        Ok(next.map(|id| (yielded, id)))
    })
    .try_collect()
    .await
}

/// Reveal hidden posts and convert system messages.
pub async fn fetch_special_post(client: &Client, post: RespPost) -> error::Result<RespPost> {
    if let Some((_, system_msg)) = post
        .action_code
        .as_ref()
        .and_then(|code| ACTION_CODE_MAP.iter().find(|(c, _)| c == code))
    {
        Ok(RespPost {
            cooked: format!("<p>系统消息：{system_msg}</p>"),
            ..post
        })
    } else if post.cooked_hidden {
        let url = format!("https://shuiyuan.sjtu.edu.cn/posts/{}/cooked.json", post.id);
        let resp: RespCooked = client.send_json(client.get(url)).await?;
        Ok(RespPost {
            cooked: format!(r#"<p style="color: gray">被折叠的内容</p>{}"#, resp.cooked),
            ..post
        })
    } else {
        Ok(post)
    }
}

#[allow(clippy::to_string_in_format_args)]
fn extract_asset_url(content: &str) -> Vec<String> {
    static FULL_URL_RE: Lazy<Regex> = Lazy::new(|| {
        let image_match = IMAGE_SUFFIXES.join("|");
        let video_match = VIDEO_SUFFIXES.join("|");
        Regex::new(&format!(
            r#"https?://shuiyuan.sjtu.edu.cn([^)'",]+.(?i:{image_match}|{video_match}))"#
        ))
        .unwrap()
    });
    static UPLOAD_URL_RE: Lazy<Regex> = Lazy::new(|| {
        let image_match = IMAGE_SUFFIXES.join("|");
        let video_match = VIDEO_SUFFIXES.join("|");
        Regex::new(&format!(
            r#"/uploads[^)'",\\]+.(?i:{image_match}|{video_match})"#
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
