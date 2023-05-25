use std::iter;
use std::path::PathBuf;

use futures::stream::FuturesUnordered;
use futures::{stream, TryStreamExt};
use lol_html::html_content::ContentType;
use lol_html::{element, RewriteStrSettings};
use once_cell::sync::Lazy;
use scraper::{CaseSensitivity, Selector};
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
        sanitize_filename::sanitize(avatar_url.split('/').last().unwrap())
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
        let filename = sanitize_filename::sanitize(emoji_path.rsplit('/').next().unwrap());
        download_manager
            .download_asset(absolute_url(emoji_path), &filename, false)
            .await?;
        filename
    } else {
        let filename = sanitize_filename::sanitize(format!("{}.png", r.emoji));
        let url = format!(
            "/images/emoji/google/{}.png",
            utils::normalize_emoji(&r.emoji)
        );
        download_manager
            .download_asset(absolute_url(&url), &filename, false)
            .await?;
        filename
    };
    let count = r.usernames.len();
    Ok((filename, count))
}

fn url_to_filename(url: &str) -> String {
    let (url, query) = url.split_once('?').unwrap_or((url, ""));
    let (url, fragment) = url.split_once('#').unwrap_or((url, ""));
    let filename = url.rsplit_once('/').map_or(url, |(_, filename)| filename);
    let (basename, ext) = filename.rsplit_once('.').unwrap_or((filename, ""));
    let mut new_name = basename.to_string();
    if !query.is_empty() {
        new_name.push('_');
        new_name.push_str(query);
    }
    if !fragment.is_empty() {
        new_name.push('_');
        new_name.push_str(fragment);
    }
    if !ext.is_empty() {
        new_name.push('.');
        new_name.push_str(ext);
    }
    sanitize_filename::sanitize(new_name)
}

fn absolute_url(url: &str) -> String {
    if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("https://shuiyuan.sjtu.edu.cn{url}")
    } else {
        url.to_string()
    }
}

pub async fn fetch_assets_of_content(
    download_manager: &DownloadManager,
    content: &str,
    anonymous: bool,
) -> error::Result<String> {
    let asset_urls: Vec<_> = extract_asset_url(content, anonymous)
        .into_iter()
        .map(|s| (s.clone(), url_to_filename(&s)))
        .collect();

    let mut content = content.to_string();
    for (url, name) in &asset_urls {
        content = content.replace(url, &format!("resources/{name}"));
    }

    let futs: FuturesUnordered<_> = asset_urls
        .into_iter()
        .map(|(url, name)| async move {
            download_manager
                .download_asset(absolute_url(&url), &name, false)
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

pub fn reify_vote(post: RespPost) -> error::Result<RespPost> {
    if post.polls.is_empty() {
        return Ok(post);
    }

    let rewrites = post.polls.iter().flat_map(|poll| {
        iter::once(element!(
            format!(
                r#"div.poll[data-poll-name="{}"] span.info-number"#,
                poll.name
            ),
            |el| {
                el.set_inner_content(&poll.voters.to_string(), ContentType::Text);
                Ok(())
            }
        ))
        .chain(poll.options.iter().map(|option| {
            element!(
                format!(
                    r#"div.poll[data-poll-name="{}"] li[data-poll-option-id="{}"]"#,
                    poll.name, option.id
                ),
                |el| {
                    el.append(&format!(" - {} 票", option.votes), ContentType::Text);
                    Ok(())
                }
            )
        }))
    });

    let cooked = lol_html::rewrite_str(
        &post.cooked,
        RewriteStrSettings {
            element_content_handlers: rewrites.collect(),
            ..RewriteStrSettings::default()
        },
    )?;
    Ok(RespPost { cooked, ..post })
}

fn parse_srcset(attr: &str) -> Vec<&str> {
    attr.split(',')
        .map(str::trim)
        .map(|s| s.rsplit_once(' ').map_or(s, |(url, _)| url))
        .collect()
}

fn filter_media(url: &str) -> bool {
    let no_query = url.rsplit_once('?').map_or(url, |(url, _)| url);
    let no_fragment = no_query.rsplit_once('#').map_or(no_query, |(url, _)| url);
    let filename = no_fragment
        .rsplit_once('/')
        .map_or(no_fragment, |(_, filename)| filename);
    let ext = filename.rsplit_once('.').map_or(filename, |(_, ext)| ext);
    VIDEO_SUFFIXES.iter().any(|&s| ext.eq_ignore_ascii_case(s))
        || IMAGE_SUFFIXES.iter().any(|&s| ext.eq_ignore_ascii_case(s))
}

#[allow(clippy::to_string_in_format_args)]
fn extract_asset_url(content: &str, anonymous: bool) -> Vec<String> {
    static A_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());
    static IMG_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("img").unwrap());
    static SOURCE_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("source").unwrap());

    let doc = scraper::Html::parse_document(content);

    let href_urls = doc
        .select(&A_SELECTOR)
        .filter_map(|a| a.value().attr("href"));
    let img_urls = doc.select(&IMG_SELECTOR).flat_map(|img| {
        let elem = img.value();
        if anonymous && elem.has_class("avatar", CaseSensitivity::AsciiCaseInsensitive) {
            return vec![];
        }

        let mut srcset_imgs = elem.attr("srcset").map_or(vec![], parse_srcset);
        if let Some(src_img) = elem.attr("src") {
            srcset_imgs.push(src_img);
        }
        srcset_imgs
    });
    let source_urls = doc
        .select(&SOURCE_SELECTOR)
        .filter_map(|source| source.value().attr("src"));

    href_urls
        .chain(img_urls)
        .chain(source_urls)
        .filter(|url| filter_media(url))
        .map(ToString::to_string)
        .collect()
}
