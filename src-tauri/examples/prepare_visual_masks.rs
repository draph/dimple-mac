use base64::{Engine, engine::general_purpose::STANDARD};
use std::{error::Error, fs, path::PathBuf};

mod localization {
    #[derive(Debug)]
    pub struct AppError;

    impl AppError {
        pub fn reason(_: &str, _: &str) -> Self {
            Self
        }
    }
}

#[path = "../src/assets.rs"]
mod assets;

fn main() -> Result<(), Box<dyn Error>> {
    let tests = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests");
    let fixtures = tests.join("fixtures/badges");
    let output = fixtures.join("visual-masks");
    fs::create_dir_all(&output)?;

    for (name, source) in [
        ("svg", fixtures.join("geometric-cross.svg")),
        ("png", fixtures.join("t14-asymmetric.png")),
        ("builtin", tests.join("../src/assets/code.svg")),
        ("text", fixtures.join("t14-text-f.png")),
    ] {
        let mask = assets::load(&source).map_err(|_| format!("E_VISUAL_MASK {name}"))?;
        if mask.width == 0 || mask.height == 0 || mask.width.max(mask.height) > 1024 {
            return Err(format!("E_VISUAL_MASK_DIMENSIONS {name}").into());
        }
        let png = STANDARD.decode(mask.png)?;
        fs::write(output.join(format!("{name}.png")), png)?;
        println!(
            "MASK {name} {}x{} bounds={:?}",
            mask.width, mask.height, mask.bounds
        );
    }
    Ok(())
}
