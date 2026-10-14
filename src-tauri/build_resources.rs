use std::{fs, io, path::Path};

pub fn without_resource_copy(mut overlay: serde_json::Value) -> serde_json::Value {
    // A null bundle removes the whole section and already disables resource copies.
    if overlay
        .get("bundle")
        .is_some_and(serde_json::Value::is_null)
    {
        return overlay;
    }
    overlay["bundle"]["resources"] = serde_json::Value::Null;
    overlay
}

pub fn copy_if_changed(source: &Path, destination: &Path) -> io::Result<()> {
    let content = fs::read(source)?;
    match fs::read(destination) {
        Ok(existing) if existing == content => return Ok(()),
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}
