use adw::prelude::*;
use gtk4 as gtk;
use gtk4::prelude::*;
use libadwaita as adw;

/// Show a destructive confirmation dialog before uninstalling an addon and
/// await the user's choice. Returns `true` only when the user clicks
/// "Uninstall"; Cancel and Escape both return `false`.
///
/// The body lists every folder that will be removed. When `is_external` is set
/// (the addon is marked as externally tracked), a reminder is appended that
/// another tool may attempt to restore the folder.
///
/// Uses `adw::MessageDialog` rather than `adw::AlertDialog` because the project
/// targets libadwaita `v1_4`; the two are equivalent for this purpose.
pub async fn confirm_uninstall(
    parent: &impl IsA<gtk::Window>,
    addon_name: &str,
    folders: &[String],
    is_external: bool,
) -> bool {
    let heading = format!("Uninstall {addon_name}?");

    let mut body =
        String::from("This will permanently delete the following folder(s) from disk:\n");
    for folder in folders {
        body.push_str(&format!("\n• {folder}"));
    }
    if is_external {
        body.push_str("\n\nThis addon is marked as externally tracked — another tool may try to restore the folder.");
    }

    let dialog = adw::MessageDialog::new(Some(parent), Some(&heading), Some(&body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("uninstall", "Uninstall");
    dialog.set_response_appearance("uninstall", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    dialog.choose_future().await == "uninstall"
}
