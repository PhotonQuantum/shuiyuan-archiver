use regex::Regex;
use tauri::plugin::Plugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_store::JsonValue;

pub struct URLSchemePlugin;

pub struct URLScheme {
    pub registered: bool,
}

impl Plugin<Wry> for URLSchemePlugin {
    fn name(&self) -> &'static str {
        "URLSchemePlugin"
    }
    fn initialize(
        &mut self,
        app: &AppHandle<Wry>,
        _config: JsonValue,
    ) -> tauri::plugin::Result<()> {
        app.manage(URLScheme {
            registered: register_deep_link(app.clone()),
        });
        Ok(())
    }
}

fn register_deep_link(handle: AppHandle<Wry>) -> bool {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        tauri_plugin_deep_link::register("discourse", move |request| {
            let re = Regex::new(r#"discourse://auth_redirect/?\?payload=(.+)"#).unwrap();
            if let Some(s) = re.captures(&request).map(|m| {
                urlencoding::decode(m.get(1).expect("no payload").as_str())
                    .expect("utf8")
                    .to_string()
            }) {
                handle.emit_all("update-token", s).unwrap()
            }
        })
        .expect("failed to register deep link handler");
        true
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    false
}
