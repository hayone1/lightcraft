//! Host storage: a page that serves LightCraft can offer its own file storage (a cloud drive, a
//! team folder) next to this device, through `window.craftStorage`. Import can then pick a photo
//! there, and Export asks where each file goes. The contract and its rules are the same for every
//! Craft app and live in `craft-web-host`; this module connects them to LightCraft's import picker
//! and export download. Without `window.craftStorage` nothing changes.
//!
//! LightCraft keeps no document files: an imported photo is copied into the library, so a photo
//! from the host is imported from its bytes, and an export is a one-off file.

use craft_web_host::storage::{self, OpenChoice, SaveChoice, SaveQueue};

use crate::backend::Backend;
use crate::safety::notice;
use crate::store::{Originals, accept_extensions, download_name};

thread_local! {
    /// Exports to the host, one at a time in the order they were made (`craft_web_host`).
    static EXPORTS: SaveQueue<()> = SaveQueue::new();
}

/// Import: the host's Open dialog when the page offers one, else (or when the user picks this
/// device there) the browser's own picker, `device`.
pub fn pick(originals: Originals, backend: Option<Backend>, ctx: egui::Context, accept: String, device: impl FnOnce() + 'static) {
    if !storage::available() {
        return device();
    }
    wasm_bindgen_futures::spawn_local(async move {
        let exts = accept_extensions(&accept);
        let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
        match storage::choose_open(Some(&exts)).await {
            Ok(OpenChoice::File { path, bytes }) => {
                let name = download_name(&path).to_string();
                crate::web::store_file(originals, backend, name, bytes, ctx).await;
            }
            // The user's own choice: the browser's picker.
            Ok(OpenChoice::Device) => device(),
            Ok(OpenChoice::Cancelled) => {}
            // A host that failed or answered something else: nothing was chosen.
            Err(e) => notice(format!("Couldn't import from the page's storage: {e}")),
        }
    });
}

/// Export: ask the host where the file goes (its storage, a download to `device`, or nowhere).
/// The export counts as started at once; the host's answer, or its failure, comes later.
pub fn export(path: &str, bytes: &[u8], device: fn(&str, &[u8]) -> Result<(), String>) -> Result<(), String> {
    if !storage::available() {
        return device(path, bytes);
    }
    let (name, bytes) = (download_name(path).to_string(), bytes.to_vec());
    EXPORTS.with(|q| {
        // Each export reports its own failure as a notice: nothing waits for the results.
        q.take_done();
        q.push(async move {
            match storage::choose_save(&name).await {
                Ok(SaveChoice::Host(to)) => {
                    if let Err(e) = storage::write(&to, &bytes).await {
                        notice(format!("{name} was not exported: {e}"));
                    }
                }
                // The user's own choice: a download to the device.
                Ok(SaveChoice::Device) => {
                    if let Err(e) = device(&name, &bytes) {
                        notice(format!("{name} was not exported: {e}"));
                    }
                }
                Ok(SaveChoice::Cancelled) => {}
                // A host that failed or answered something else: not exported.
                Err(e) => notice(format!("{name} was not exported: {e}")),
            }
        })
    });
    Ok(())
}
