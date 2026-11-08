pub const BUILTINS: &[(&str, &[u8])] = &[
    ("code", include_bytes!("../../src/assets/code.svg")),
    ("terminal", include_bytes!("../../src/assets/terminal.svg")),
    ("database", include_bytes!("../../src/assets/database.svg")),
    ("branch", include_bytes!("../../src/assets/branch.svg")),
    ("bug", include_bytes!("../../src/assets/bug.svg")),
    ("wrench", include_bytes!("../../src/assets/wrench.svg")),
    ("hammer", include_bytes!("../../src/assets/hammer.svg")),
    ("scissors", include_bytes!("../../src/assets/scissors.svg")),
    ("ruler", include_bytes!("../../src/assets/ruler.svg")),
    ("key", include_bytes!("../../src/assets/key.svg")),
    ("arrow-up", include_bytes!("../../src/assets/arrow-up.svg")),
    (
        "arrow-down",
        include_bytes!("../../src/assets/arrow-down.svg"),
    ),
    (
        "arrow-left",
        include_bytes!("../../src/assets/arrow-left.svg"),
    ),
    (
        "arrow-right",
        include_bytes!("../../src/assets/arrow-right.svg"),
    ),
    ("refresh", include_bytes!("../../src/assets/refresh.svg")),
    ("play", include_bytes!("../../src/assets/play.svg")),
    ("pause", include_bytes!("../../src/assets/pause.svg")),
    ("music", include_bytes!("../../src/assets/music.svg")),
    ("camera", include_bytes!("../../src/assets/camera.svg")),
    (
        "microphone",
        include_bytes!("../../src/assets/microphone.svg"),
    ),
];

pub fn builtin(id: &str) -> Option<&'static [u8]> {
    BUILTINS
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, bytes)| *bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_embedded_assets_and_every_icon_normalizes() {
        let catalog: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../src/assets/icons.json")).unwrap();
        assert_eq!(catalog.len(), BUILTINS.len());
        for icon in catalog {
            let id = icon["id"].as_str().unwrap();
            let bytes = builtin(id).unwrap();
            let mask = crate::assets::svg_mask(bytes).unwrap();
            assert!(mask.width > 0 && mask.width <= 1024);
            assert!(mask.height > 0 && mask.height <= 1024);
            assert!(!mask.opaque);
            assert_eq!(icon["source"], format!("src/assets/{id}.svg"));
        }
        assert!(builtin("../../arbitrary.svg").is_none());
    }
}
