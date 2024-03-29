use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Datelike, Local, Utc};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;

#[derive(Debug, Deserialize)]
pub struct RespTopic {
    pub title: String,
    pub category_id: usize,
    pub tags: Vec<String>,
    pub post_stream: PostStream,
    pub posts_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct PostStream {
    pub posts: Vec<RespPost>,
    pub stream: Option<Vec<u32>>,
}

#[derive(Debug, Deserialize)]
pub struct RespPosts {
    pub post_stream: PostStream,
}

#[derive(Debug, Deserialize)]
pub struct RespCategory {
    pub category: RespCategoryInner,
}

#[derive(Debug, Deserialize)]
pub struct RespCategoryInner {
    #[serde(flatten)]
    pub inner: Category,
    pub parent_category_id: Option<usize>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Post {
    pub name: String,
    pub number: usize,
    pub username: String,
    pub created_at: String,
    pub created_at_display: String,
    pub content: String,
    pub likes: usize,
    pub reply_to: Option<usize>,
    pub emojis: HashMap<String, usize>,
    pub avatar: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
pub struct RespPost {
    pub id: usize,
    pub post_number: usize,
    pub name: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub cooked: String,
    #[serde(default)]
    pub cooked_hidden: bool,
    pub actions_summary: Vec<Actions>,
    pub reply_to_post_number: Option<usize>,
    pub retorts: Vec<RespRetort>,
    pub avatar_template: String,
    pub action_code: Option<String>,
    #[serde(default)]
    pub polls: Vec<RespPoll>,
}

#[derive(Debug, Deserialize)]
pub struct RespPoll {
    pub name: String,
    pub options: Vec<RespPollOption>,
    pub voters: usize,
}

#[derive(Debug, Deserialize)]
pub struct RespPollOption {
    pub id: String,
    pub html: String,
    #[serde(default)]
    pub votes: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct RespRetort {
    pub usernames: Vec<String>,
    pub emoji: String,
}

#[derive(Debug, Deserialize)]
pub struct RespCooked {
    pub cooked: String,
}

#[derive(Debug, Deserialize)]
pub struct Actions {
    pub id: usize,
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Topic<'a> {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub categories: Vec<Category>,
    pub tags: Vec<String>,
    pub posts: &'a [Post],
    pub page: usize,
    pub total_pages: usize,
    pub prev_page: Option<String>,
    // can be "index"
    pub next_page: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Params<'a> {
    #[serde(flatten)]
    pub topic: Topic<'a>,
    pub app_version: String,
    pub year: i32,
}

impl<'a> From<Topic<'a>> for Params<'a> {
    fn from(t: Topic<'a>) -> Self {
        Self {
            topic: t,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            year: Local::now().year(),
        }
    }
}

#[typeshare]
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TopicMeta {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub categories: Vec<Category>,
    pub tags: Vec<String>,
    pub post_ids: Vec<u32>,
}
