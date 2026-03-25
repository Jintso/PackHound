use std::path::PathBuf;

use adw::prelude::*;
use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::config::Config;

/// Expand a leading `~` to the user's home directory.
/// If the home directory can't be determined, the path is returned unchanged.
fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if path == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(path)
}

/// Show the preferences window, transient to `parent`.
pub fn show_settings(parent: &adw::ApplicationWindow) {
    let config = Config::load().unwrap_or_default();

    let window = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title("Preferences")
        .default_width(480)
        .build();

    // ── Header bar with explicit Save button ─────────────────────────────────

    let header = adw::HeaderBar::new();
    let save_button = gtk::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    header.pack_end(&save_button);

    // ── Content ──────────────────────────────────────────────────────────────

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(12);
    content.set_margin_bottom(18);

    // WoW Installation group
    let wow_group = adw::PreferencesGroup::builder()
        .title("WoW Installation")
        .description("Path to your WoW root directory (contains _retail_/, _classic_/, etc.)")
        .build();

    let detect_button = gtk::Button::with_label("Auto-detect");
    detect_button.add_css_class("flat");
    wow_group.set_header_suffix(Some(&detect_button));

    let path_row = adw::EntryRow::builder()
        .title("WoW Root Path")
        .text(
            config
                .wow_root
                .as_deref()
                .and_then(|p| p.to_str())
                .unwrap_or(""),
        )
        .build();
    wow_group.add(&path_row);

    {
        let path_row = path_row.clone();
        detect_button.connect_clicked(move |_| match Config::detect_wow_root() {
            Some(path) => path_row.set_text(path.to_str().unwrap_or("")),
            None => path_row.set_text(""),
        });
    }

    content.append(&wow_group);

    // GitHub group
    let gh_group = adw::PreferencesGroup::builder()
        .title("GitHub")
        .description("Personal access token for higher API rate limits (60 → 5,000 req/hr)")
        .build();

    let token_row = adw::PasswordEntryRow::builder()
        .title("Personal Access Token")
        .text(config.github_token.as_deref().unwrap_or(""))
        .build();
    gh_group.add(&token_row);

    content.append(&gh_group);

    // CurseForge group
    let cf_group = adw::PreferencesGroup::builder()
        .title("CurseForge")
        .description("API key from console.curseforge.com — required for CurseForge addons")
        .build();

    let cf_key_row = adw::PasswordEntryRow::builder()
        .title("API Key")
        .text(config.curseforge_api_key.as_deref().unwrap_or(""))
        .build();
    cf_group.add(&cf_key_row);

    content.append(&cf_group);

    // ── Layout ───────────────────────────────────────────────────────────────
    // No scroll window — let the content set the window height naturally.

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&content));

    window.set_content(Some(&toolbar_view));

    // ── Save button: persist config and close ────────────────────────────────
    {
        let path_row = path_row.clone();
        let token_row = token_row.clone();
        let cf_key_row = cf_key_row.clone();
        let window_ref = window.clone();
        save_button.connect_clicked(move |_| {
            let wow_root = {
                let text = path_row.text();
                if text.is_empty() {
                    None
                } else {
                    Some(expand_tilde(text.as_str()))
                }
            };
            let github_token = {
                let text = token_row.text();
                if text.is_empty() {
                    None
                } else {
                    Some(text.to_string())
                }
            };
            let curseforge_api_key = {
                let text = cf_key_row.text();
                if text.is_empty() {
                    None
                } else {
                    Some(text.to_string())
                }
            };
            if let Err(e) = (Config {
                wow_root,
                github_token,
                curseforge_api_key,
            })
            .save()
            {
                eprintln!("Failed to save config: {e}");
            }
            window_ref.close();
        });
    }

    window.present();
}
