use std::collections::HashMap;

use fake::faker::name::en::Name;
use fake::Fake;
use once_cell::sync::Lazy;
use regex::{Captures, Regex};

use crate::models::Post;

// Minimum trimmed length for an ascii username to be replaced globally in a post on anonymous mode.
const MIN_ASCII_NAME_LENGTH: usize = 5;
// Minimum trimmed length for a unicode username to be replaced globally in a post on anonymous mode.
const MIN_UNICODE_NAME_LENGTH: usize = 2;

static RE_MENTION: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<a class="mention" href="/u/.*">@(.*)</a>"#).unwrap());
static RE_QUOTE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<img .* src=".*" class="avatar"> (.*):</div>"#).unwrap());
static RE_FROM: Lazy<Regex> = Lazy::new(|| Regex::new(r#"来自 (.*)</a>"#).unwrap());

pub fn collect_anonymous_names<'a>(
    posts: impl IntoIterator<Item = &'a Post> + Clone,
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

pub fn mask_username_in_cooked(fake_name_map: &HashMap<String, String>, mut s: String) -> String {
    #[allow(clippy::type_complexity)]
    let re_f: &[(_, fn(&str) -> String)] = &[
        (&RE_MENTION, |fake_name| {
            format!(r#"<a class="mention">@{fake_name}</a>"#)
        }),
        (&RE_QUOTE, |fake_name| format!(r#" {fake_name}:</div>"#)),
        (&RE_FROM, |fake_name| format!(r#"来自 {fake_name}</a>"#)),
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
