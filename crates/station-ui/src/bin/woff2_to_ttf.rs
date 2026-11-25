use anyhow::{Context, Result, bail};
use std::io::Cursor;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(src_dir) = args.next().map(PathBuf::from) else {
        bail!("usage: woff2_to_ttf <src-fonts-dir> <dst-fonts-dir>");
    };
    let Some(dst_dir) = args.next().map(PathBuf::from) else {
        bail!("usage: woff2_to_ttf <src-fonts-dir> <dst-fonts-dir>");
    };

    std::fs::create_dir_all(&dst_dir)
        .with_context(|| format!("create {}", dst_dir.display()))?;

    let mut count = 0usize;
    let mut entries: Vec<_> = std::fs::read_dir(&src_dir)
        .with_context(|| format!("read {}", src_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("woff2") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid font stem")?;
        let raw = std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let ttf = woff2_patched::convert_woff2_to_ttf(&mut Cursor::new(&raw))
            .map_err(|e| anyhow::anyhow!("decode {}: {e:?}", path.display()))?;
        let out = dst_dir.join(format!("{stem}.ttf"));
        std::fs::write(&out, &ttf).with_context(|| format!("write {}", out.display()))?;
        count += 1;
    }

    println!("decoded {count} woff2 fonts into {}", dst_dir.display());
    Ok(())
}
