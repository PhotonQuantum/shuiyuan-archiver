use std::io::Cursor;
use std::path::Path;

use handlebars::no_escape;
use handlebars::Handlebars;
use handlebars::{handlebars_helper, html_escape};
use once_cell::sync::Lazy;

use crate::error;

const TEMPLATE: &str = include_str!("../../templates/index.hbs");
const RESOURCES: &[u8] = include_bytes!("../../resources.tar.gz");

handlebars_helper!(escape: | x: String | html_escape( & x));

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

pub fn extract_resources(to: impl AsRef<Path>) -> error::Result<()> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(Cursor::new(RESOURCES)));
    archive.unpack(to)?;
    Ok(())
}
