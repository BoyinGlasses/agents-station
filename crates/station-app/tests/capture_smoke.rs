#[test]
fn capture_scene_smoke_writes_non_black_png_and_exits_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("smoke.png");
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_agents-station"))
        .args(["--capture-scene", "smoke", "--out", out.to_str().unwrap()])
        .status()
        .expect("spawn agents-station");
    assert!(status.success(), "capture exited with {status}");
    let img = image::open(&out).expect("valid PNG").to_rgba8();
    assert!(img.width() >= 640 && img.height() >= 360);
    let non_black = img
        .pixels()
        .filter(|p| p[0] > 10 || p[1] > 10 || p[2] > 10)
        .count();
    let unique_buckets: std::collections::HashSet<(u8, u8, u8)> = img
        .pixels()
        .map(|p| (p[0] / 32, p[1] / 32, p[2] / 32))
        .collect();
    assert!(
        non_black > (img.width() * img.height() / 10) as usize,
        "image appears black"
    );
    assert!(unique_buckets.len() >= 3, "image appears flat single color");
}
