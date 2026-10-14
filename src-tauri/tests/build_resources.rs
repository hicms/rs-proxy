#[path = "../build_resources.rs"]
mod build_resources;

use build_resources::copy_if_changed;
use std::fs;

#[test]
fn resource_override_preserves_other_settings_and_bundle_deletion() {
    use serde_json::json;
    let deletion = json!({"bundle": null, "build": {"devUrl": null}});
    assert_eq!(
        build_resources::without_resource_copy(deletion.clone()),
        deletion
    );
    let overlay = json!({"bundle": {"resources": ["custom"], "active": false}, "build": null});
    assert_eq!(
        build_resources::without_resource_copy(overlay),
        json!({"bundle": {"resources": null, "active": false}, "build": null})
    );
    assert_eq!(
        build_resources::without_resource_copy(json!({})),
        json!({"bundle": {"resources": null}})
    );
}

#[test]
fn missing_and_changed_resources_are_copied() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let destination = directory.path().join("bundle/driver.sys");
    fs::write(&source, b"release one").unwrap();
    copy_if_changed(&source, &destination).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"release one");
    fs::write(&source, b"release two").unwrap();
    copy_if_changed(&source, &destination).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"release two");
}

#[test]
fn identical_locked_resources_are_preserved_but_changed_ones_fail() {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source");
    let destination = directory.path().join("driver.sys");
    fs::write(&source, b"official release").unwrap();
    fs::write(&destination, b"official release").unwrap();
    let original_time = fs::metadata(&destination).unwrap().modified().unwrap();
    let _loaded = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&destination)
        .unwrap();
    copy_if_changed(&source, &destination).unwrap();
    assert_eq!(
        fs::metadata(&destination).unwrap().modified().unwrap(),
        original_time
    );
    fs::write(&source, b"different release").unwrap();
    assert_eq!(
        copy_if_changed(&source, &destination)
            .unwrap_err()
            .raw_os_error(),
        Some(32)
    );
    assert_eq!(fs::read(&destination).unwrap(), b"official release");
}
