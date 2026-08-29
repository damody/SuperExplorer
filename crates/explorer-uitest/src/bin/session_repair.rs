use std::{env, fs, path::PathBuf};

use anyhow::{Context as _, Result, bail};
use explorer_common::RoadmapLimits;
use explorer_model::PersistedSessionEnvelope;

fn main() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(input) = args.next().map(PathBuf::from) else {
        bail!(
            "usage: explorer-session-repair <input> <output> [--enable-immersive-native-context-menus]"
        );
    };
    let Some(output) = args.next().map(PathBuf::from) else {
        bail!("missing repaired-session output path");
    };
    let enable_immersive = match args.next() {
        None => false,
        Some(flag) if flag == "--enable-immersive-native-context-menus" => true,
        Some(flag) => bail!("unexpected argument: {}", flag.to_string_lossy()),
    };
    if args.next().is_some() {
        bail!("unexpected extra argument");
    }

    // Deserialize without calling `decode`: this tool exists specifically to recover a payload
    // whose envelope checksum became stale while leaving the bounded model validation authoritative
    // when `PersistedSessionEnvelope::new` constructs the replacement.
    let bytes = fs::read(&input).with_context(|| format!("read {}", input.display()))?;
    let stale: PersistedSessionEnvelope =
        serde_json::from_slice(&bytes).context("deserialize recoverable session envelope")?;
    let mut payload = stale.payload;
    if enable_immersive {
        for tab in &mut payload.tabs {
            tab.view_settings.immersive_native_context_menus = true;
        }
    }
    let repaired = PersistedSessionEnvelope::new(
        stale.write_generation.saturating_add(1),
        stale.provenance,
        payload,
        RoadmapLimits::default(),
    )?;
    let encoded = repaired.encode_pretty(RoadmapLimits::default())?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output parent {}", parent.display()))?;
    }
    fs::write(&output, encoded).with_context(|| format!("write {}", output.display()))?;
    Ok(())
}
