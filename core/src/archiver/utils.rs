use std::collections::hash_map::DefaultHasher;
use std::fmt::Display;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Local, Utc};
use html2text::render::text_renderer::TrivialDecorator;
use once_cell::sync::Lazy;
use regex::Regex;

pub fn ceil_div(x: usize, y: usize) -> usize {
    x / y + usize::from(x % y != 0)
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

pub fn normalize_emoji(emoji: &str) -> impl Display + '_ {
    // For toned emoji, see
    // https://github.com/discourse/discourse/blob/c85e3e80838d75d8eec132267e2903d729f12aa4/app/models/emoji.rb#L104
    static EMOJI_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(.+):t([1-6])").unwrap());
    EMOJI_RE.replace_all(emoji.trim_matches(':'), "$1/$2")
}

pub fn summarize(content: &str) -> String {
    html2text::parse(content.as_bytes())
        .render(120, TrivialDecorator::new())
        .into_string()
}

pub fn datetime_to_display(datetime: &DateTime<Utc>) -> String {
    datetime
        .with_timezone(&Local)
        .format("%Y年%m月%d日 %H:%M")
        .to_string()
}
