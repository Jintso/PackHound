mod addon;
mod config;
mod curseforge;
mod github;
mod ui;
mod update;

use adw::prelude::*;
use libadwaita as adw;

const APP_ID: &str = "com.github.packhound";

fn main() {
    // A Tokio runtime must be active for reqwest (Hyper DNS) to work, even
    // when HTTP calls are awaited on the GLib main loop via spawn_future_local.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");
    let _guard = rt.enter();

    let app = adw::Application::builder().application_id(APP_ID).build();

    app.connect_activate(ui::window::build_ui);
    app.run();
}
