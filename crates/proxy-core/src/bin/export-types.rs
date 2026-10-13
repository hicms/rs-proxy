use proxy_core::{
    config::Config,
    model::{AutostartState, ProcessIcon, ProxyTestResult, Snapshot},
};
use ts_rs::TS;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/bindings");
    Config::export_all_to(&out)?;
    Snapshot::export_all_to(&out)?;
    ProcessIcon::export_all_to(&out)?;
    AutostartState::export_all_to(&out)?;
    ProxyTestResult::export_all_to(&out)?;
    Ok(())
}
