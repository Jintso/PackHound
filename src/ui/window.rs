use adw::prelude::*;
use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

use super::{add_dialog, settings};
use crate::{
    addon::{AddonSource, AddonState, WowFlavor, registry::AddonRegistry},
    config::Config,
    update::{check_all_updates, check_app_update},
};

pub fn build_ui(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("PackHound")
        .default_width(800)
        .default_height(600)
        .build();

    let toast_overlay = adw::ToastOverlay::new();

    // ── Header bar ───────────────────────────────────────────────────────────
    let header = adw::HeaderBar::new();

    // Centred title widget (larger, cleaner look)
    let title_widget = adw::WindowTitle::new("PackHound", "");
    header.set_title_widget(Some(&title_widget));

    // Add Addon button — flat style matching other header buttons
    let add_content = adw::ButtonContent::builder()
        .icon_name("list-add-symbolic")
        .label("Add Addon")
        .build();
    let add_button = gtk::Button::new();
    add_button.set_child(Some(&add_content));
    add_button.set_tooltip_text(Some("Add Addon"));
    add_button.set_focusable(false);
    header.pack_end(&add_button);

    let update_all_button = gtk::Button::with_label("Update All");
    update_all_button.add_css_class("suggested-action");
    update_all_button.set_visible(false);
    update_all_button.set_focusable(false);
    header.pack_end(&update_all_button);

    let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_button.set_tooltip_text(Some("Preferences"));
    settings_button.set_focusable(false);

    // Hide-externals toggle
    let hide_ext_btn = gtk::ToggleButton::new();
    hide_ext_btn.set_icon_name("eye-not-looking-symbolic");
    hide_ext_btn.set_tooltip_text(Some("Hide externally tracked addons"));
    hide_ext_btn.set_focusable(false);

    // Sort menu button — uses a popover instead of GtkDropDown to avoid
    // a GTK4 rendering bug that clips dropdown left rounded edges.
    let sort_menu = gtk::gio::Menu::new();
    sort_menu.append(Some("Name A\u{2192}Z"), Some("win.sort-order::az"));
    sort_menu.append(Some("Name Z\u{2192}A"), Some("win.sort-order::za"));
    sort_menu.append(Some("Unsorted"), Some("win.sort-order::none"));

    let sort_button = gtk::MenuButton::new();
    sort_button.set_icon_name("view-sort-ascending-symbolic");
    sort_button.set_tooltip_text(Some("Sort order"));
    sort_button.set_menu_model(Some(&sort_menu));
    sort_button.set_focusable(false);

    // Sort state action — default "az"
    let sort_action = gtk::gio::SimpleAction::new_stateful(
        "sort-order",
        Some(&String::static_variant_type()),
        &"az".to_variant(),
    );
    window.add_action(&sort_action);

    header.pack_start(&settings_button);
    header.pack_start(&hide_ext_btn);
    header.pack_start(&sort_button);

    // ── Main stack ───────────────────────────────────────────────────────────
    let stack = gtk::Stack::new();
    repopulate_stack(
        &stack,
        &update_all_button,
        &window,
        &toast_overlay,
        &hide_ext_btn,
        &sort_action,
    );

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&stack));

    toast_overlay.set_child(Some(&toolbar_view));
    window.set_content(Some(&toast_overlay));

    // ── Settings ─────────────────────────────────────────────────────────────
    {
        let window_ref = window.clone();
        settings_button.connect_clicked(move |_| settings::show_settings(&window_ref));
    }

    // ── Add Addon ─────────────────────────────────────────────────────────────
    {
        let window_ref = window.clone();
        let stack_ref = stack.clone();
        let overlay_ref = toast_overlay.clone();
        let update_all_ref = update_all_button.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        add_button.connect_clicked(move |_| {
            let stack = stack_ref.clone();
            let overlay = overlay_ref.clone();
            let update_all = update_all_ref.clone();
            let window_cb = window_ref.clone();
            let hide = hide_ref.clone();
            let sort = sort_ref.clone();
            add_dialog::show_add_dialog(&window_ref, move |addon_name| {
                repopulate_stack(&stack, &update_all, &window_cb, &overlay, &hide, &sort);
                overlay.add_toast(
                    adw::Toast::builder()
                        .title(format!("{addon_name} installed successfully"))
                        .timeout(3)
                        .build(),
                );
            });
        });
    }

    // ── Update All ───────────────────────────────────────────────────────────
    {
        let stack_ref = stack.clone();
        let overlay_ref = toast_overlay.clone();
        let update_all_ref = update_all_button.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        update_all_button.connect_clicked(move |btn| {
            run_update_all(
                btn,
                &stack_ref,
                &overlay_ref,
                &update_all_ref,
                &window_ref,
                &hide_ref,
                &sort_ref,
            );
        });
    }

    // ── Filter / Sort controls refresh ───────────────────────────────────────
    {
        let stack_ref = stack.clone();
        let overlay_ref = toast_overlay.clone();
        let update_all_ref = update_all_button.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        hide_ext_btn.connect_toggled(move |_| {
            repopulate_stack(
                &stack_ref,
                &update_all_ref,
                &window_ref,
                &overlay_ref,
                &hide_ref,
                &sort_ref,
            );
        });
    }
    {
        let stack_ref = stack.clone();
        let overlay_ref = toast_overlay.clone();
        let update_all_ref = update_all_button.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        sort_action.connect_activate(move |action, param| {
            if let Some(val) = param.and_then(|p| p.get::<String>()) {
                action.set_state(&val.to_variant());
            }
            repopulate_stack(
                &stack_ref,
                &update_all_ref,
                &window_ref,
                &overlay_ref,
                &hide_ref,
                &sort_ref,
            );
        });
    }

    window.present();

    // ── Check for app updates (non-blocking) ─────────────────────────────────
    {
        let overlay_ref = toast_overlay.clone();
        gtk::glib::spawn_future_local(async move {
            let token = Config::load().ok().and_then(|c| c.github_token);
            if let Ok(Some(update)) = check_app_update(token.as_deref()).await {
                let toast = adw::Toast::builder()
                    .title(format!("PackHound {} available", update.version))
                    .button_label("Download")
                    .timeout(10)
                    .build();

                let url = update.release_url;
                toast.connect_button_clicked(move |_| {
                    let _ = gtk::gio::AppInfo::launch_default_for_uri(
                        &url,
                        gtk::gio::AppLaunchContext::NONE,
                    );
                });

                overlay_ref.add_toast(toast);
            }
        });
    }

    // ── Auto-check addons on launch (non-blocking) ───────────────────────────
    {
        let stack_ref = stack.clone();
        let overlay_ref = toast_overlay.clone();
        let update_all_ref = update_all_button.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        gtk::glib::spawn_future_local(async move {
            let token = Config::load().ok().and_then(|c| c.github_token);
            match check_all_updates(token.as_deref()).await {
                Ok(result) => {
                    repopulate_stack(
                        &stack_ref,
                        &update_all_ref,
                        &window_ref,
                        &overlay_ref,
                        &hide_ref,
                        &sort_ref,
                    );
                    let n = result.updates_available;
                    if n > 0 {
                        overlay_ref.add_toast(
                            adw::Toast::builder()
                                .title(format!(
                                    "{n} update{} available",
                                    if n == 1 { "" } else { "s" }
                                ))
                                .timeout(5)
                                .build(),
                        );
                    }
                    for warning in &result.warnings {
                        overlay_ref.add_toast(
                            adw::Toast::builder()
                                .title(warning.as_str())
                                .timeout(10)
                                .build(),
                        );
                    }
                }
                Err(e) => {
                    overlay_ref.add_toast(
                        adw::Toast::builder()
                            .title(format!("Update check failed: {e}"))
                            .timeout(10)
                            .build(),
                    );
                }
            }
        });
    }
}

fn run_update_all(
    btn: &gtk::Button,
    stack: &gtk::Stack,
    overlay: &adw::ToastOverlay,
    update_all_btn: &gtk::Button,
    window: &adw::ApplicationWindow,
    hide_ext_btn: &gtk::ToggleButton,
    sort_action: &gtk::gio::SimpleAction,
) {
    btn.set_sensitive(false);
    let stack = stack.clone();
    let overlay = overlay.clone();
    let update_all_btn = update_all_btn.clone();
    let window = window.clone();
    let hide = hide_ext_btn.clone();
    let sort = sort_action.clone();

    gtk::glib::spawn_future_local(async move {
        let token = Config::load().ok().and_then(|c| c.github_token);
        match do_update_all(token.as_deref()).await {
            Ok(n) => {
                repopulate_stack(&stack, &update_all_btn, &window, &overlay, &hide, &sort);
                overlay.add_toast(
                    adw::Toast::builder()
                        .title(format!(
                            "{n} addon{} updated",
                            if n == 1 { "" } else { "s" }
                        ))
                        .timeout(3)
                        .build(),
                );
            }
            Err(e) => {
                overlay.add_toast(
                    adw::Toast::builder()
                        .title(format!("Update failed: {e}"))
                        .timeout(5)
                        .build(),
                );
                update_all_btn.set_sensitive(true);
            }
        }
    });
}

async fn do_update_all(token: Option<&str>) -> anyhow::Result<usize> {
    use crate::{
        addon::installer::{download_to_temp, extract_addon},
        github::{
            client::GitHubClient,
            parse_repo_url,
            resolver::{AssetSelection, select_asset_with_hint},
        },
    };

    let config = Config::load()?;
    let client = GitHubClient::new(token)?;
    let http = reqwest::Client::builder()
        .user_agent(concat!("packhound/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let mut registry = AddonRegistry::load()?;
    let mut updated = 0;

    for addon in registry.addons.iter_mut() {
        if addon.state != AddonState::UpdateAvailable {
            continue;
        }
        let latest = match &addon.latest_version {
            Some(v) => v.clone(),
            None => continue,
        };
        if !addon.source.has_remote() || addon.externally_tracked {
            continue;
        }

        let result: anyhow::Result<(Vec<String>, Option<String>)> = async {
            let addons_dir = config.addons_dir(&addon.flavor).ok_or_else(|| {
                anyhow::anyhow!(
                    "WoW path not configured for {}",
                    addon.flavor.display_name()
                )
            })?;
            match &addon.source {
                AddonSource::GitHub { url } => {
                    let (owner, repo) = parse_repo_url(url)?;
                    let release = client.fetch_latest_release(&owner, &repo).await?;
                    let asset = match select_asset_with_hint(
                        &release.assets,
                        &addon.flavor,
                        Some(&addon.name),
                    ) {
                        AssetSelection::Found(a) => a,
                        _ => anyhow::bail!("No suitable asset for {}", addon.name),
                    };
                    let tmp = download_to_temp(&http, &asset.download_url, |_, _| {}).await?;
                    let folders = extract_addon(tmp.path(), &addons_dir)?;
                    Ok((folders, release.published_at))
                }
                AddonSource::CurseForge { mod_id, .. } => {
                    let cf_api_key = config
                        .curseforge_api_key
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("CurseForge API key not configured"))?;
                    let cf_client = crate::curseforge::client::CurseForgeClient::new(cf_api_key)?;
                    let files = cf_client.list_files(*mod_id, Some(&addon.flavor)).await?;
                    let file = files.into_iter().next().ok_or_else(|| {
                        anyhow::anyhow!("No files found on CurseForge for {}", addon.name)
                    })?;
                    let download_url = file.resolve_download_url();
                    let tmp = download_to_temp(&http, &download_url, |_, _| {}).await?;
                    let folders = extract_addon(tmp.path(), &addons_dir)?;
                    // Update the source with new file_id
                    addon.source = AddonSource::CurseForge {
                        mod_id: *mod_id,
                        file_id: file.id,
                        url: addon.source.url().unwrap_or("").to_string(),
                    };
                    Ok((folders, None))
                }
                AddonSource::None => anyhow::bail!("No remote source"),
            }
        }
        .await;

        match result {
            Ok((new_folders, pub_date)) => {
                addon.installed_version = latest;
                addon.state = AddonState::Installed;
                addon.release_date = pub_date;
                if !new_folders.is_empty() {
                    addon.folders = new_folders;
                }
                updated += 1;
            }
            Err(e) => eprintln!("Failed to update {}: {e}", addon.name),
        }
    }

    registry.save()?;
    Ok(updated)
}

async fn do_update_single(
    addon_name: String,
    flavor: WowFlavor,
    source: AddonSource,
    token: Option<&str>,
) -> anyhow::Result<String> {
    use crate::{
        addon::installer::{download_to_temp, extract_addon},
        github::{
            client::GitHubClient,
            parse_repo_url,
            resolver::{AssetSelection, select_asset_with_hint},
        },
    };

    let config = Config::load()?;
    let addons_dir = config
        .addons_dir(&flavor)
        .ok_or_else(|| anyhow::anyhow!("WoW path not configured for {}", flavor.display_name()))?;

    let http = reqwest::Client::builder()
        .user_agent(concat!("packhound/", env!("CARGO_PKG_VERSION")))
        .build()?;

    match &source {
        AddonSource::GitHub { url } => {
            let client = GitHubClient::new(token)?;
            let (owner, repo) = parse_repo_url(url)?;
            let release = client.fetch_latest_release(&owner, &repo).await?;
            let asset = match select_asset_with_hint(&release.assets, &flavor, Some(&addon_name)) {
                AssetSelection::Found(a) => a,
                AssetSelection::Ambiguous(_) => anyhow::bail!(
                    "Multiple assets match. Try reinstalling via Add Addon to choose one."
                ),
                AssetSelection::NotFound => anyhow::bail!(
                    "No zip asset found for '{}' in the latest release.",
                    flavor.display_name()
                ),
            };

            let tmp = download_to_temp(&http, &asset.download_url, |_, _| {}).await?;
            let new_folders = extract_addon(tmp.path(), &addons_dir)?;

            let mut registry = AddonRegistry::load()?;
            if let Some(entry) = registry
                .addons
                .iter_mut()
                .find(|a| a.name == addon_name && a.flavor == flavor)
            {
                entry.installed_version = release.tag_name.clone();
                entry.state = AddonState::Installed;
                entry.release_date = release.published_at;
                if !new_folders.is_empty() {
                    entry.folders = new_folders;
                }
            }
            registry.save()?;
            Ok(release.tag_name)
        }
        AddonSource::CurseForge { mod_id, .. } => {
            let cf_api_key = config
                .curseforge_api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("CurseForge API key not configured"))?;
            let cf_client = crate::curseforge::client::CurseForgeClient::new(cf_api_key)?;
            let files = cf_client.list_files(*mod_id, Some(&flavor)).await?;
            let file = files
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No files found on CurseForge for {addon_name}"))?;
            let download_url = file.resolve_download_url();
            let tmp = download_to_temp(&http, &download_url, |_, _| {}).await?;
            let new_folders = extract_addon(tmp.path(), &addons_dir)?;

            let mut registry = AddonRegistry::load()?;
            if let Some(entry) = registry
                .addons
                .iter_mut()
                .find(|a| a.name == addon_name && a.flavor == flavor)
            {
                entry.installed_version = file.display_name.clone();
                entry.state = AddonState::Installed;
                entry.source = AddonSource::CurseForge {
                    mod_id: *mod_id,
                    file_id: file.id,
                    url: source.url().unwrap_or("").to_string(),
                };
                if !new_folders.is_empty() {
                    entry.folders = new_folders;
                }
            }
            registry.save()?;
            Ok(file.display_name)
        }
        AddonSource::None => anyhow::bail!("No remote source for this addon"),
    }
}

// ── Stack population ──────────────────────────────────────────────────────────

fn repopulate_stack(
    stack: &gtk::Stack,
    update_all_btn: &gtk::Button,
    window: &adw::ApplicationWindow,
    overlay: &adw::ToastOverlay,
    hide_ext_btn: &gtk::ToggleButton,
    sort_action: &gtk::gio::SimpleAction,
) {
    while let Some(child) = stack.first_child() {
        stack.remove(&child);
    }

    let config = Config::load().unwrap_or_default();
    let registry = AddonRegistry::load().unwrap_or_default();
    let untracked = registry.scan_untracked(&config);

    let hide_externals = hide_ext_btn.is_active();
    let sort_order = sort_action
        .state()
        .and_then(|s| s.get::<String>())
        .unwrap_or_else(|| "az".to_string());

    let has_updates = registry
        .addons()
        .iter()
        .any(|a| a.state == AddonState::UpdateAvailable && !a.externally_tracked);
    update_all_btn.set_visible(has_updates);

    // ── Empty state ──────────────────────────────────────────────────────────
    let empty_page = adw::StatusPage::builder()
        .icon_name("package-x-generic-symbolic")
        .title("No Addons Installed")
        .description("Click + to add an addon from a GitHub URL.")
        .build();
    stack.add_named(&empty_page, Some("empty"));

    // ── Addon list ───────────────────────────────────────────────────────────
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);
    // No boxed-list — avoids rounded-edge clipping during scroll

    // Collect + filter tracked addons
    let mut addons: Vec<&crate::addon::Addon> = registry
        .addons()
        .iter()
        .filter(|a| !(hide_externals && a.externally_tracked))
        .collect();

    // Sort — updates pending always float to top, then by chosen order
    let updates_first = |a: &&crate::addon::Addon, b: &&crate::addon::Addon| {
        let a_up = a.state == AddonState::UpdateAvailable && !a.externally_tracked;
        let b_up = b.state == AddonState::UpdateAvailable && !b.externally_tracked;
        b_up.cmp(&a_up)
    };
    match sort_order.as_str() {
        "az" => addons.sort_by(|a, b| {
            updates_first(a, b).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        }),
        "za" => addons.sort_by(|a, b| {
            updates_first(a, b).then_with(|| b.name.to_lowercase().cmp(&a.name.to_lowercase()))
        }),
        _ => addons.sort_by(|a, b| updates_first(a, b)),
    }

    for addon in addons {
        let toc = config
            .addons_dir(&addon.flavor)
            .and_then(|dir| crate::addon::toc::read_toc(&dir, &addon.name, &addon.flavor));

        let display_name = toc
            .as_ref()
            .and_then(|t| t.title.as_deref())
            .unwrap_or(&addon.name);

        let toc_version = toc.as_ref().and_then(|t| t.version.as_deref());
        let display_version = toc_version.unwrap_or(addon.installed_version.as_str());

        let date_str = addon
            .release_date
            .as_deref()
            .map(format_date_short)
            .unwrap_or_default();

        let subtitle = if date_str.is_empty() {
            format!("{} • {}", addon.flavor, display_version)
        } else {
            format!("{} • {} • {}", addon.flavor, display_version, date_str)
        };

        let row = adw::ActionRow::new();
        row.set_title(display_name);
        row.set_subtitle(&subtitle);

        // Deps tooltip
        if let Some(t) = &toc
            && !t.dependencies.is_empty()
        {
            row.set_tooltip_text(Some(&format!("Requires: {}", t.dependencies.join(", "))));
        }

        if addon.externally_tracked {
            let lbl = gtk::Label::new(Some("External"));
            lbl.add_css_class("dim-label");
            lbl.set_valign(gtk::Align::Center);
            row.add_suffix(&lbl);
        } else if addon.state == AddonState::UpdateAvailable {
            let update_btn = gtk::Button::with_label("Update");
            update_btn.add_css_class("flat");
            update_btn.set_valign(gtk::Align::Center);

            {
                let addon_name = addon.name.clone();
                let flavor = addon.flavor.clone();
                let source = addon.source.clone();
                let stack_ref = stack.clone();
                let overlay_ref = overlay.clone();
                let update_all_ref = update_all_btn.clone();
                let window_ref = window.clone();
                let update_btn_ref = update_btn.clone();
                let hide_ref = hide_ext_btn.clone();
                let sort_ref = sort_action.clone();
                update_btn.connect_clicked(move |_| {
                    update_btn_ref.set_sensitive(false);
                    let addon_name = addon_name.clone();
                    let flavor = flavor.clone();
                    let source = source.clone();
                    let stack = stack_ref.clone();
                    let overlay = overlay_ref.clone();
                    let update_all = update_all_ref.clone();
                    let window_cb = window_ref.clone();
                    let btn = update_btn_ref.clone();
                    let hide = hide_ref.clone();
                    let sort = sort_ref.clone();
                    gtk::glib::spawn_future_local(async move {
                        let token = Config::load().ok().and_then(|c| c.github_token);
                        match do_update_single(addon_name.clone(), flavor, source, token.as_deref())
                            .await
                        {
                            Ok(_) => {
                                repopulate_stack(
                                    &stack,
                                    &update_all,
                                    &window_cb,
                                    &overlay,
                                    &hide,
                                    &sort,
                                );
                                overlay.add_toast(
                                    adw::Toast::builder()
                                        .title(format!("{addon_name} updated"))
                                        .timeout(3)
                                        .build(),
                                );
                            }
                            Err(e) => {
                                btn.set_sensitive(true);
                                overlay.add_toast(
                                    adw::Toast::builder()
                                        .title(format!("Update failed: {e}"))
                                        .timeout(5)
                                        .build(),
                                );
                            }
                        }
                    });
                });
            }
            row.add_suffix(&update_btn);
        } else {
            row.add_suffix(&state_label(&addon.state));
        }

        attach_context_menu(
            &row,
            addon.name.clone(),
            addon.flavor.clone(),
            addon.source.clone(),
            addon.externally_tracked,
            stack,
            update_all_btn,
            window,
            overlay,
            hide_ext_btn,
            sort_action,
        );

        list_box.append(&row);
    }

    // Untracked addons
    let total_tracked = registry.addons().len();
    let show_list = !registry.addons().is_empty() || !untracked.is_empty() || total_tracked > 0;

    for (flavor, name) in untracked {
        let row = adw::ActionRow::new();
        row.set_title(&name);
        row.set_subtitle(&format!("{} • Not tracked", flavor));

        let track_btn = gtk::Button::with_label("Track");
        track_btn.add_css_class("flat");
        track_btn.set_valign(gtk::Align::Center);

        {
            let window_ref = window.clone();
            let stack_ref = stack.clone();
            let overlay_ref = overlay.clone();
            let update_all_ref = update_all_btn.clone();
            let window_for_cb = window.clone();
            let name_clone = name.clone();
            let flavor_clone = flavor.clone();
            let hide_ref = hide_ext_btn.clone();
            let sort_ref = sort_action.clone();
            track_btn.connect_clicked(move |_| {
                let stack = stack_ref.clone();
                let overlay = overlay_ref.clone();
                let update_all = update_all_ref.clone();
                let win_cb = window_for_cb.clone();
                let hide = hide_ref.clone();
                let sort = sort_ref.clone();
                add_dialog::show_track_dialog(
                    &window_ref,
                    name_clone.clone(),
                    flavor_clone.clone(),
                    move || repopulate_stack(&stack, &update_all, &win_cb, &overlay, &hide, &sort),
                );
            });
        }

        // Right-click on untracked rows → same Track action
        attach_untracked_context_menu(
            &row,
            name.clone(),
            flavor.clone(),
            stack,
            update_all_btn,
            window,
            overlay,
            hide_ext_btn,
            sort_action,
        );

        row.add_suffix(&track_btn);
        list_box.append(&row);
    }

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&list_box));
    scrolled.set_vexpand(true);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.set_margin_start(12);
    content_box.set_margin_end(12);
    content_box.set_margin_top(12);
    content_box.set_margin_bottom(12);
    content_box.append(&scrolled);

    stack.add_named(&content_box, Some("list"));
    let show = show_list
        && (!registry.addons().is_empty() || !registry.scan_untracked(&config).is_empty());
    stack.set_visible_child_name(if show { "list" } else { "empty" });
}

// ── Context menus ─────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn attach_context_menu(
    row: &adw::ActionRow,
    addon_name: String,
    flavor: WowFlavor,
    source: AddonSource,
    externally_tracked: bool,
    stack: &gtk::Stack,
    update_all_btn: &gtk::Button,
    window: &adw::ApplicationWindow,
    overlay: &adw::ToastOverlay,
    hide_ext_btn: &gtk::ToggleButton,
    sort_action: &gtk::gio::SimpleAction,
) {
    let popover = gtk::Popover::new();
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    menu_box.set_margin_start(4);
    menu_box.set_margin_end(4);
    menu_box.set_margin_top(4);
    menu_box.set_margin_bottom(4);

    // "Open in Browser" — only when a URL is set
    if let Some(browse_url) = source.url() {
        let btn = menu_item("Open in Browser");
        let url = browse_url.to_string();
        let popover_ref = popover.clone();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
        });
        menu_box.append(&btn);
        menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    }

    // "Change Source URL"
    {
        let btn = menu_item("Change Source URL");
        let window_ref = window.clone();
        let stack_ref = stack.clone();
        let overlay_ref = overlay.clone();
        let update_all_ref = update_all_btn.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        let popover_ref = popover.clone();
        let name = addon_name.clone();
        let flav = flavor.clone();
        let current_url = source.url().unwrap_or("").to_string();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            let stack = stack_ref.clone();
            let overlay = overlay_ref.clone();
            let update_all = update_all_ref.clone();
            let win_cb = window_ref.clone();
            let hide = hide_ref.clone();
            let sort = sort_ref.clone();
            add_dialog::show_edit_url_dialog(
                &window_ref,
                name.clone(),
                flav.clone(),
                current_url.clone(),
                move || repopulate_stack(&stack, &update_all, &win_cb, &overlay, &hide, &sort),
            );
        });
        menu_box.append(&btn);
    }

    // "Mark / Unmark as Externally Tracked"
    {
        let label = if externally_tracked {
            "Unmark as Externally Tracked"
        } else {
            "Mark as Externally Tracked"
        };
        let btn = menu_item(label);
        let stack_ref = stack.clone();
        let overlay_ref = overlay.clone();
        let update_all_ref = update_all_btn.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        let popover_ref = popover.clone();
        let name = addon_name.clone();
        let flav = flavor.clone();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            let mut reg = AddonRegistry::load().unwrap_or_default();
            if let Some(a) = reg
                .addons
                .iter_mut()
                .find(|a| a.name == name && a.flavor == flav)
            {
                a.externally_tracked = !a.externally_tracked;
            }
            let _ = reg.save();
            repopulate_stack(
                &stack_ref,
                &update_all_ref,
                &window_ref,
                &overlay_ref,
                &hide_ref,
                &sort_ref,
            );
        });
        menu_box.append(&btn);
    }

    menu_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    // "Remove from List"
    {
        let btn = menu_item("Remove from List");
        btn.add_css_class("error");
        let stack_ref = stack.clone();
        let overlay_ref = overlay.clone();
        let update_all_ref = update_all_btn.clone();
        let window_ref = window.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        let popover_ref = popover.clone();
        let name = addon_name.clone();
        let flav = flavor.clone();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            let mut reg = AddonRegistry::load().unwrap_or_default();
            reg.addons.retain(|a| !(a.name == name && a.flavor == flav));
            let _ = reg.save();
            repopulate_stack(
                &stack_ref,
                &update_all_ref,
                &window_ref,
                &overlay_ref,
                &hide_ref,
                &sort_ref,
            );
        });
        menu_box.append(&btn);
    }

    popover.set_child(Some(&menu_box));
    popover.set_parent(row);

    {
        let p = popover.clone();
        row.connect_destroy(move |_| p.unparent());
    }

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let p = popover.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        p.set_pointing_to(Some(&rect));
        p.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(gesture);
}

#[allow(clippy::too_many_arguments)]
fn attach_untracked_context_menu(
    row: &adw::ActionRow,
    folder_name: String,
    flavor: WowFlavor,
    stack: &gtk::Stack,
    update_all_btn: &gtk::Button,
    window: &adw::ApplicationWindow,
    overlay: &adw::ToastOverlay,
    hide_ext_btn: &gtk::ToggleButton,
    sort_action: &gtk::gio::SimpleAction,
) {
    let popover = gtk::Popover::new();
    let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    menu_box.set_margin_start(4);
    menu_box.set_margin_end(4);
    menu_box.set_margin_top(4);
    menu_box.set_margin_bottom(4);

    {
        let btn = menu_item("Track Addon");
        let window_ref = window.clone();
        let stack_ref = stack.clone();
        let overlay_ref = overlay.clone();
        let update_all_ref = update_all_btn.clone();
        let hide_ref = hide_ext_btn.clone();
        let sort_ref = sort_action.clone();
        let popover_ref = popover.clone();
        let name = folder_name.clone();
        let flav = flavor.clone();
        btn.connect_clicked(move |_| {
            popover_ref.popdown();
            let stack = stack_ref.clone();
            let overlay = overlay_ref.clone();
            let update_all = update_all_ref.clone();
            let win_cb = window_ref.clone();
            let hide = hide_ref.clone();
            let sort = sort_ref.clone();
            add_dialog::show_track_dialog(&window_ref, name.clone(), flav.clone(), move || {
                repopulate_stack(&stack, &update_all, &win_cb, &overlay, &hide, &sort)
            });
        });
        menu_box.append(&btn);
    }

    popover.set_child(Some(&menu_box));
    popover.set_parent(row);

    {
        let p = popover.clone();
        row.connect_destroy(move |_| p.unparent());
    }

    let gesture = gtk::GestureClick::new();
    gesture.set_button(3);
    let p = popover.clone();
    gesture.connect_pressed(move |gesture, _, x, y| {
        let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
        p.set_pointing_to(Some(&rect));
        p.popup();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    row.add_controller(gesture);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn menu_item(label: &str) -> gtk::Button {
    let btn = gtk::Button::with_label(label);
    btn.set_has_frame(false);
    btn.set_halign(gtk::Align::Fill);
    btn.set_hexpand(true);
    btn
}

fn state_label(state: &AddonState) -> gtk::Label {
    let (text, css_class) = match state {
        AddonState::Installed => ("Up to date", "success"),
        AddonState::UpdateAvailable => ("Update available", "warning"),
        AddonState::Installing => ("Installing…", "accent"),
        AddonState::CheckingForUpdates => ("Checking…", "dim-label"),
    };
    let label = gtk::Label::new(Some(text));
    label.add_css_class(css_class);
    label.set_valign(gtk::Align::Center);
    label
}

/// Format an ISO 8601 timestamp as `"Mon YYYY"`, e.g. `"Jan 2025"`.
fn format_date_short(iso: &str) -> String {
    let date = iso.split('T').next().unwrap_or(iso);
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() >= 2 {
        let month = match parts[1] {
            "01" => "Jan",
            "02" => "Feb",
            "03" => "Mar",
            "04" => "Apr",
            "05" => "May",
            "06" => "Jun",
            "07" => "Jul",
            "08" => "Aug",
            "09" => "Sep",
            "10" => "Oct",
            "11" => "Nov",
            "12" => "Dec",
            _ => parts[1],
        };
        if let Some(year) = parts.first() {
            return format!("{month} {year}");
        }
    }
    iso.to_string()
}
