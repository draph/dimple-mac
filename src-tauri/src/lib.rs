#[derive(serde::Serialize)]
struct Session {
    version: &'static str,
}

#[tauri::command]
fn get_session() -> Session {
    Session {
        version: env!("CARGO_PKG_VERSION"),
    }
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_session])
        .run(tauri::generate_context!())
        .expect("Failed to start Folderify Desktop");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serializes_the_running_version() {
        let value = serde_json::to_value(get_session()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })
        );
    }
}
