use adw::prelude::*;
use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use crate::{
    addon::{
        Addon, AddonSource, AddonState, WowFlavor,
        installer::{download_to_temp, extract_addon},
        registry::AddonRegistry,
    },
    config::Config,
    curseforge::client::{self as cf, CurseForgeClient},
    github::{
        client::{GitHubClient, ReleaseAsset},
        parse_repo_url,
        resolver::{AssetSelection, select_asset_with_hint},
    },
};

// ── Add Addon dialog ─────────────────────────────────────────────────────────

/// Show the "Add Addon" dialog, transient to `parent`.
/// `on_installed(addon_name)` is called after a successful install so the
/// caller can refresh the list and show a toast.
pub fn show_add_dialog(parent: &adw::ApplicationWindow, on_installed: impl Fn(&str) + 'static) {
    let on_installed = std::sync::Arc::new(on_installed);
    let dialog = adw::Window::builder()
        .title("Add Addon")
        .default_width(420)
        .modal(true)
        .transient_for(parent)
        .build();

    let toolbar_view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar_view.add_top_bar(&header);

    // ── Form ─────────────────────────────────────────────────────────────────

    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(6);
    form.set_margin_bottom(18);

    let url_group = adw::PreferencesGroup::new();
    let url_row = adw::EntryRow::builder()
        .title("GitHub or CurseForge URL")
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    url_group.add(&url_row);
    form.append(&url_group);

    let flavor_group = adw::PreferencesGroup::new();
    let flavor_row = adw::ComboRow::builder().title("WoW Version").build();
    let flavor_model = gtk::StringList::new(&[
        WowFlavor::Retail.display_name(),
        WowFlavor::ClassicEra.display_name(),
        WowFlavor::Classic.display_name(),
    ]);
    flavor_row.set_model(Some(&flavor_model));
    flavor_group.add(&flavor_row);
    form.append(&flavor_group);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_visible(false);
    form.append(&progress_bar);

    let status_label = gtk::Label::new(None);
    status_label.set_wrap(true);
    status_label.add_css_class("dim-label");
    status_label.set_visible(false);
    form.append(&status_label);

    let install_button = gtk::Button::with_label("Install");
    install_button.add_css_class("suggested-action");
    install_button.add_css_class("pill");
    install_button.set_halign(gtk::Align::Center);
    install_button.set_sensitive(false);
    form.append(&install_button);

    toolbar_view.set_content(Some(&form));
    dialog.set_content(Some(&toolbar_view));

    // ── Validate URL as the user types ───────────────────────────────────────
    {
        let install_button = install_button.clone();
        url_row.connect_changed(move |row| {
            let text = row.text();
            let url = text.as_str();
            let valid = parse_repo_url(url).is_ok() || cf::is_curseforge_url(url);
            install_button.set_sensitive(valid);
        });
    }

    // ── Install button ───────────────────────────────────────────────────────
    {
        let url_row = url_row.clone();
        let flavor_row = flavor_row.clone();
        let progress_bar = progress_bar.clone();
        let status_label = status_label.clone();
        let install_button = install_button.clone();
        let dialog = dialog.clone();

        install_button.connect_clicked(move |btn| {
            let url = url_row.text().to_string();
            let flavor = flavor_index_to_enum(flavor_row.selected());

            set_form_sensitive(&url_row, &flavor_row, btn, false);
            progress_bar.set_visible(true);
            progress_bar.set_fraction(0.0);
            status_label.set_visible(true);
            status_label.set_text("Fetching release info…");

            let progress_bar = progress_bar.clone();
            let status_label = status_label.clone();
            let dialog = dialog.clone();
            let url_row = url_row.clone();
            let flavor_row = flavor_row.clone();
            let btn = btn.clone();
            let parent_for_picker = dialog.clone();

            let on_installed = on_installed.clone();
            gtk::glib::spawn_future_local(async move {
                match run_install(
                    &url,
                    &flavor,
                    parent_for_picker,
                    &progress_bar,
                    &status_label,
                )
                .await
                {
                    Ok(name) => {
                        on_installed(&name);
                        dialog.close();
                    }
                    Err(e) => {
                        status_label.remove_css_class("dim-label");
                        status_label.add_css_class("error");
                        status_label.set_text(&format!("Error: {e}"));
                        progress_bar.set_visible(false);
                        set_form_sensitive(&url_row, &flavor_row, &btn, true);
                    }
                }
            });
        });
    }

    dialog.present();
}

async fn run_install(
    url: &str,
    flavor: &WowFlavor,
    parent: adw::Window,
    progress_bar: &gtk::ProgressBar,
    status_label: &gtk::Label,
) -> anyhow::Result<String> {
    if cf::is_curseforge_url(url) {
        run_install_curseforge(url, flavor, progress_bar, status_label).await
    } else {
        run_install_github(url, flavor, parent, progress_bar, status_label).await
    }
}

async fn run_install_github(
    repo_url: &str,
    flavor: &WowFlavor,
    parent: adw::Window,
    progress_bar: &gtk::ProgressBar,
    status_label: &gtk::Label,
) -> anyhow::Result<String> {
    let (owner, repo) = parse_repo_url(repo_url)?;
    let config = Config::load()?;
    let client = GitHubClient::new(config.github_token.as_deref())?;

    status_label.set_text("Fetching latest release…");
    let release = client.fetch_latest_release(&owner, &repo).await?;

    let asset = match select_asset_with_hint(&release.assets, flavor, None) {
        AssetSelection::Found(a) => a,
        AssetSelection::NotFound => anyhow::bail!(
            "No zip asset found for '{}' in the latest release.",
            flavor.display_name()
        ),
        AssetSelection::Ambiguous(assets) => pick_asset_via_dialog(parent, assets).await?,
    };

    let addons_dir = config.addons_dir(flavor).ok_or_else(|| {
        anyhow::anyhow!(
            "WoW path for '{}' not configured. Open Preferences first.",
            flavor.display_name()
        )
    })?;

    status_label.set_text(&format!("Downloading {}…", asset.name));

    let http = reqwest::Client::builder()
        .user_agent(concat!("addon-manager/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let total_size = asset.size;
    let pb = progress_bar.clone();
    let tmp = download_to_temp(&http, &asset.download_url, move |downloaded, _| {
        if total_size > 0 {
            pb.set_fraction(downloaded as f64 / total_size as f64);
        } else {
            pb.pulse();
        }
    })
    .await?;

    status_label.set_text("Extracting…");
    let addon_folders = extract_addon(tmp.path(), &addons_dir)?;
    let primary_name = if addon_folders.is_empty() {
        repo.clone()
    } else {
        crate::addon::find_primary_folder(&addon_folders)
    };

    let source = AddonSource::GitHub {
        url: repo_url.to_string(),
    };

    // Update registry (replace existing entry for same source+flavor)
    let mut registry = AddonRegistry::load().unwrap_or_default();
    registry
        .addons
        .retain(|a| !(a.source == source && a.flavor == *flavor));
    registry.addons.push(Addon {
        name: primary_name.clone(),
        folders: addon_folders,
        source,
        flavor: flavor.clone(),
        installed_version: release.tag_name,
        latest_version: None,
        release_date: release.published_at,
        state: AddonState::Installed,
        externally_tracked: false,
    });
    registry.save()?;

    Ok(primary_name)
}

async fn run_install_curseforge(
    url: &str,
    flavor: &WowFlavor,
    progress_bar: &gtk::ProgressBar,
    status_label: &gtk::Label,
) -> anyhow::Result<String> {
    let config = Config::load()?;
    let api_key = config.curseforge_api_key.as_deref().ok_or_else(|| {
        anyhow::anyhow!("CurseForge API key not configured. Add it in Preferences first.")
    })?;

    let slug = cf::parse_curseforge_url(url)?;

    status_label.set_text("Looking up addon on CurseForge…");
    let client = CurseForgeClient::new(api_key)?;
    let cf_mod = client.find_mod_by_slug(&slug).await?;

    status_label.set_text("Fetching files…");
    let files = client.list_files(cf_mod.id, Some(flavor)).await?;

    let file = files.into_iter().next().ok_or_else(|| {
        anyhow::anyhow!(
            "No files found for '{}' on CurseForge for {}.",
            cf_mod.name,
            flavor.display_name()
        )
    })?;

    let download_url = file.resolve_download_url();

    let addons_dir = config.addons_dir(flavor).ok_or_else(|| {
        anyhow::anyhow!(
            "WoW path for '{}' not configured. Open Preferences first.",
            flavor.display_name()
        )
    })?;

    status_label.set_text(&format!("Downloading {}…", file.file_name));
    progress_bar.pulse();

    let http = reqwest::Client::builder()
        .user_agent(concat!("addon-manager/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let pb = progress_bar.clone();
    let tmp = download_to_temp(&http, &download_url, move |_, _| {
        pb.pulse();
    })
    .await?;

    status_label.set_text("Extracting…");
    let addon_folders = extract_addon(tmp.path(), &addons_dir)?;
    let primary_name = if addon_folders.is_empty() {
        slug.clone()
    } else {
        crate::addon::find_primary_folder(&addon_folders)
    };

    let source = AddonSource::CurseForge {
        mod_id: cf_mod.id,
        file_id: file.id,
        url: url.to_string(),
    };

    let mut registry = AddonRegistry::load().unwrap_or_default();
    registry
        .addons
        .retain(|a| a.name != primary_name || a.flavor != *flavor);
    registry.addons.push(Addon {
        name: primary_name.clone(),
        folders: addon_folders,
        source,
        flavor: flavor.clone(),
        installed_version: file.display_name,
        latest_version: None,
        release_date: None,
        state: AddonState::Installed,
        externally_tracked: false,
    });
    registry.save()?;

    Ok(primary_name)
}

/// Show the asset picker dialog and wait for the user to choose one.
/// Returns `Err` if the dialog is cancelled.
async fn pick_asset_via_dialog(
    parent: adw::Window,
    assets: Vec<ReleaseAsset>,
) -> anyhow::Result<ReleaseAsset> {
    let (tx, rx) = tokio::sync::oneshot::channel::<ReleaseAsset>();
    let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    show_asset_picker_dialog(&parent, assets, move |asset| {
        if let Some(tx) = tx.lock().unwrap().take() {
            let _ = tx.send(asset);
        }
    });
    rx.await
        .map_err(|_| anyhow::anyhow!("Asset selection cancelled"))
}

// ── Asset picker dialog ───────────────────────────────────────────────────────

/// Show a dialog listing the given zip assets and call `on_picked` when the
/// user selects one. Used when multiple assets match for a given release.
pub fn show_asset_picker_dialog(
    parent: &adw::Window,
    assets: Vec<ReleaseAsset>,
    on_picked: impl Fn(ReleaseAsset) + 'static,
) {
    let dialog = adw::Window::builder()
        .title("Select Asset")
        .default_width(400)
        .modal(true)
        .transient_for(parent)
        .build();

    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 12);
    vbox.set_margin_start(18);
    vbox.set_margin_end(18);
    vbox.set_margin_top(6);
    vbox.set_margin_bottom(18);

    let hint = gtk::Label::new(Some(
        "Multiple zip files found in this release. Select the one to install:",
    ));
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    hint.set_xalign(0.0);
    vbox.append(&hint);

    let group = adw::PreferencesGroup::new();
    let on_picked = std::sync::Arc::new(on_picked);

    for asset in assets {
        let row = adw::ActionRow::new();
        row.set_title(&asset.name);
        row.set_activatable(true);

        let dialog_ref = dialog.clone();
        let cb = on_picked.clone();
        let a = asset.clone();
        row.connect_activated(move |_| {
            cb(a.clone());
            dialog_ref.close();
        });
        group.add(&row);
    }
    vbox.append(&group);

    toolbar.set_content(Some(&vbox));
    dialog.set_content(Some(&toolbar));
    dialog.present();
}

// ── Track dialog ─────────────────────────────────────────────────────────────

/// Show a dialog to optionally connect an existing (untracked) addon folder
/// to a GitHub repository for update checking.
pub fn show_track_dialog(
    parent: &adw::ApplicationWindow,
    folder_name: String,
    flavor: WowFlavor,
    on_tracked: impl Fn() + 'static,
) {
    let dialog = adw::Window::builder()
        .title("Track Addon")
        .default_width(420)
        .modal(true)
        .transient_for(parent)
        .build();

    let header = adw::HeaderBar::new();
    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);

    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(6);
    form.set_margin_bottom(18);

    let info_group = adw::PreferencesGroup::builder()
        .title(&folder_name)
        .description(format!("WoW version: {}", flavor.display_name()))
        .build();
    form.append(&info_group);

    let url_group = adw::PreferencesGroup::new();
    let url_row = adw::EntryRow::builder()
        .title("GitHub or CurseForge URL (optional)")
        .input_purpose(gtk::InputPurpose::Url)
        .build();
    url_group.add(&url_row);
    form.append(&url_group);

    let hint = gtk::Label::new(Some("Leave blank to track without update checking."));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    form.append(&hint);

    let track_button = gtk::Button::with_label("Track Addon");
    track_button.add_css_class("suggested-action");
    track_button.add_css_class("pill");
    track_button.set_halign(gtk::Align::Center);
    form.append(&track_button);

    toolbar_view.set_content(Some(&form));
    dialog.set_content(Some(&toolbar_view));

    let on_tracked = std::sync::Arc::new(on_tracked);
    let dialog_ref = dialog.clone();
    track_button.connect_clicked(move |btn| {
        let url_text = url_row.text().to_string();
        let url = url_text.trim().to_string();
        let folder_name = folder_name.clone();
        let flavor = flavor.clone();
        let on_tracked = on_tracked.clone();
        let dialog_ref = dialog_ref.clone();
        btn.set_sensitive(false);

        gtk::glib::spawn_future_local(async move {
            let source = resolve_source_from_url(&url, &flavor).await;
            let state = if source.has_remote() {
                AddonState::UpdateAvailable
            } else {
                AddonState::Installed
            };

            let mut registry = AddonRegistry::load().unwrap_or_default();
            registry
                .addons
                .retain(|a| !(a.name == folder_name && a.flavor == flavor));
            registry.addons.push(Addon {
                name: folder_name.clone(),
                folders: vec![folder_name.clone()],
                source,
                flavor,
                installed_version: "unknown".to_string(),
                latest_version: None,
                release_date: None,
                state,
                externally_tracked: false,
            });
            if let Err(e) = registry.save() {
                eprintln!("Failed to save registry: {e}");
            }
            on_tracked();
            dialog_ref.close();
        });
    });

    dialog.present();
}

// ── Edit URL dialog ───────────────────────────────────────────────────────────

/// Show a dialog to edit the source URL for an already-tracked addon.
pub fn show_edit_url_dialog(
    parent: &adw::ApplicationWindow,
    addon_name: String,
    flavor: WowFlavor,
    current_url: String,
    on_saved: impl Fn() + 'static,
) {
    let dialog = adw::Window::builder()
        .title("Change Source URL")
        .default_width(420)
        .modal(true)
        .transient_for(parent)
        .build();

    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.set_margin_start(18);
    form.set_margin_end(18);
    form.set_margin_top(6);
    form.set_margin_bottom(18);

    let info_group = adw::PreferencesGroup::builder()
        .title(&addon_name)
        .description(format!("WoW version: {}", flavor.display_name()))
        .build();
    form.append(&info_group);

    let url_group = adw::PreferencesGroup::new();
    let url_row = adw::EntryRow::builder()
        .title("GitHub or CurseForge URL")
        .input_purpose(gtk::InputPurpose::Url)
        .text(&current_url)
        .build();
    url_group.add(&url_row);
    form.append(&url_group);

    let save_button = gtk::Button::with_label("Save");
    save_button.add_css_class("suggested-action");
    save_button.add_css_class("pill");
    save_button.set_halign(gtk::Align::Center);
    form.append(&save_button);

    toolbar.set_content(Some(&form));
    dialog.set_content(Some(&toolbar));

    let on_saved = std::sync::Arc::new(on_saved);
    let dialog_ref = dialog.clone();
    save_button.connect_clicked(move |btn| {
        let new_url = url_row.text().trim().to_string();
        let addon_name = addon_name.clone();
        let flavor = flavor.clone();
        let on_saved = on_saved.clone();
        let dialog_ref = dialog_ref.clone();
        btn.set_sensitive(false);

        gtk::glib::spawn_future_local(async move {
            let new_source = resolve_source_from_url(&new_url, &flavor).await;
            let mut registry = AddonRegistry::load().unwrap_or_default();
            if let Some(a) = registry
                .addons
                .iter_mut()
                .find(|a| a.name == addon_name && a.flavor == flavor)
            {
                a.source = new_source;
            }
            if let Err(e) = registry.save() {
                eprintln!("Failed to save registry: {e}");
            }
            on_saved();
            dialog_ref.close();
        });
    });

    dialog.present();
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a URL string into the appropriate `AddonSource`.
/// For CurseForge URLs this performs an async API lookup to resolve the mod ID.
/// Returns `AddonSource::None` if the URL is empty or unrecognised.
async fn resolve_source_from_url(url: &str, flavor: &WowFlavor) -> AddonSource {
    if url.is_empty() {
        return AddonSource::None;
    }
    if cf::is_curseforge_url(url) {
        if let Ok(slug) = cf::parse_curseforge_url(url)
            && let Some(api_key) = Config::load()
                .unwrap_or_default()
                .curseforge_api_key
                .as_deref()
            && let Ok(client) = CurseForgeClient::new(api_key)
            && let Ok(cf_mod) = client.find_mod_by_slug(&slug).await
        {
            let file_id = client
                .list_files(cf_mod.id, Some(flavor))
                .await
                .ok()
                .and_then(|files| files.first().map(|f| f.id))
                .unwrap_or(0);
            return AddonSource::CurseForge {
                mod_id: cf_mod.id,
                file_id,
                url: url.to_string(),
            };
        }
        // Couldn't resolve — fall back to None so we don't lose the intent
        eprintln!("Could not resolve CurseForge URL: {url}");
        return AddonSource::None;
    }
    // Default: treat as GitHub
    AddonSource::GitHub {
        url: url.to_string(),
    }
}

fn flavor_index_to_enum(index: u32) -> WowFlavor {
    match index {
        1 => WowFlavor::ClassicEra,
        2 => WowFlavor::Classic,
        _ => WowFlavor::Retail,
    }
}

fn set_form_sensitive(
    url_row: &adw::EntryRow,
    flavor_row: &adw::ComboRow,
    btn: &gtk::Button,
    sensitive: bool,
) {
    url_row.set_sensitive(sensitive);
    flavor_row.set_sensitive(sensitive);
    btn.set_sensitive(sensitive);
}
