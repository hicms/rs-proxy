use anyhow::{Context, Result};
use std::{env, ffi::OsStr, path::PathBuf};
use tauri_utils::{config::BundleResources, platform::Target, resources::ResourcePaths};

mod build_resources;

fn main() -> Result<()> {
    let target = Target::from_triple(&env::var("TARGET")?);
    let (mut config, _) = tauri_utils::config::parse::read_from(target, &env::current_dir()?)?;
    let original_overlay = env::var("TAURI_CONFIG").ok();
    let overlay: serde_json::Value = original_overlay
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_else(|| serde_json::json!({}));
    json_patch::merge(&mut config, &overlay);
    let config: tauri_utils::config::Config = serde_json::from_value(config)?;
    let out = PathBuf::from(env::var_os("OUT_DIR").context("missing OUT_DIR")?);
    let destination = out
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new("build")))
        .and_then(|path| path.parent())
        .context("cannot resolve Cargo output directory")?;
    if let Some(resources) = config.bundle.resources {
        let paths = match &resources {
            BundleResources::List(list) => ResourcePaths::new(list, true),
            BundleResources::Map(map) => ResourcePaths::from_map(map, true),
        };
        let mut paths = paths.iter();
        for resource in paths.by_ref() {
            let resource = resource?;
            let output = destination.join(resource.target());
            println!("cargo:rerun-if-changed={}", resource.path().display());
            build_resources::copy_if_changed(resource.path(), &output).with_context(|| {
                format!(
                    "copy resource {} to {}; close its users before replacing a changed file",
                    resource.path().display(),
                    output.display()
                )
            })?;
        }
        for path in paths.rerun_if_changed() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    // Loaded Windows drivers cannot be overwritten, even with identical bytes.
    // Only this build process skips Tauri's second copy; bundling keeps its config.
    let overlay = serde_json::to_string(&build_resources::without_resource_copy(overlay))?;
    // This build script is single-threaded; set before Tauri starts any work.
    unsafe { env::set_var("TAURI_CONFIG", overlay) };
    let attributes = tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
    );
    let result = tauri_build::try_build(attributes);
    unsafe {
        if let Some(original) = original_overlay {
            env::set_var("TAURI_CONFIG", original);
        } else {
            env::remove_var("TAURI_CONFIG");
        }
    }
    result.context("build desktop resources")
}
