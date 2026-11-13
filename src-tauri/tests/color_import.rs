use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn real_worker_imports_high_entropy_rgba_above_the_old_stdout_limit() {
    let path =
        Temp(std::env::temp_dir().join(format!("dimple-color-worker-{}.png", std::process::id())));
    let mut state = 0x12345678u32;
    let rgba = (0..1024 * 1024 * 4)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    {
        let file = fs::File::create(&path.0).unwrap();
        let mut encoder = png::Encoder::new(file, 1024, 1024);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    assert!(fs::metadata(&path.0).unwrap().len() < 10 * 1024 * 1024);
    let mut child = Command::new(env!("CARGO_BIN_EXE_Dimple"))
        .arg("--target-check-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&serde_json::json!({"Inspect":[path.0]})).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.len() > 4 * 1024 * 1024);
    assert!(output.stdout.len() < 6 * 1024 * 1024);
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let mask = &response["Ok"]["mask"];
    assert_eq!(mask["width"], 1024);
    assert_eq!(mask["height"], 1024);
    assert_eq!(mask["opaque"], false);
    assert!(mask["png"].as_str().unwrap().len() > 4 * 1024 * 1024);
}
