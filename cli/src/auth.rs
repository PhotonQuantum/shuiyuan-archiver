use anyhow::{anyhow, Result};
use console::style;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use tracing::warn;

use sa_core::client::{oauth_url, token_from_payload};
use sa_core::re_exports::rsa;

use crate::APP_ID;

pub fn auth(no_open: bool) -> Result<()> {
    let key =
        rsa::RsaPrivateKey::new(&mut rand::thread_rng(), 2048).expect("generate rsa private key");
    let url = oauth_url(&APP_ID, &key, false);
    if !no_open && webbrowser::open(&url).is_ok() {
        eprintln!("A browser window should have been opened.\n\
            Please log in and authorize the app. Then copy the authenticate key from the website and paste it here.");
    } else {
        eprintln!("Please open the following URL in a browser and log in to authorize the app. Then copy the authenticate key from the website and paste it here.");
        eprintln!("{url}");
    }
    let payload: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!(
            "{} {}",
            style("?").green().bold(),
            style("Paste the authenticate key").bold()
        ))
        .interact_text()
        .unwrap();
    match token_from_payload(&payload, &key) {
        Ok(token) => {
            eprintln!("\nUse the following token to authenticate in the future.");
            eprintln!("{} {token}", style("Token:").bold());
            Ok(())
        }
        Err(e) => {
            warn!(?e, "Failed to get token from payload.");
            Err(anyhow!("This is not a valid token."))
        }
    }
}
