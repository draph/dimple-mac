use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-Hans")]
    ZhHans,
}

impl Locale {
    fn from_resolved(language: &str) -> Self {
        match language {
            "zh-Hans" => Self::ZhHans,
            _ => Self::En,
        }
    }

    pub fn current() -> Self {
        use objc2_foundation::{NSArray, NSBundle, NSString};
        let bundle = NSBundle::mainBundle();
        let languages = if bundle.localizations().is_empty() {
            // Unbundled development runs have no Info.plist localization declaration.
            let supported = NSArray::from_retained_slice(&[
                NSString::from_str("en"),
                NSString::from_str("zh-Hans"),
            ]);
            NSBundle::preferredLocalizationsFromArray(&supported)
        } else {
            bundle.preferredLocalizations()
        };
        languages
            .firstObject()
            .map(|value| Self::from_resolved(&value.to_string()))
            .unwrap_or(Self::En)
    }

    pub fn messages(self) -> BTreeMap<String, String> {
        let english = include_str!("../../src/locales/en.json");
        let mut messages: BTreeMap<String, String> =
            serde_json::from_str(english).expect("Invalid English localization resource");
        if self == Self::ZhHans {
            messages.extend(
                serde_json::from_str::<BTreeMap<String, String>>(include_str!(
                    "../../src/locales/zh-Hans.json"
                ))
                .expect("Invalid Chinese localization resource"),
            );
        }
        messages
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub params: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_languages_are_bounded_and_default_to_english() {
        assert_eq!(Locale::from_resolved("zh-Hans"), Locale::ZhHans);
        for language in ["en", "en-GB", "fr", "", "zh-Hant"] {
            assert_eq!(Locale::from_resolved(language), Locale::En);
        }
    }

    #[test]
    fn structured_errors_do_not_serialize_raw_messages() {
        let mut error = AppError {
            code: "native_connection_failed",
            params: BTreeMap::new(),
            target_id: None,
        };
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "code": "native_connection_failed", "params": {}
            })
        );
        error.target_id = Some("target-1".into());
        error.params.insert("count".into(), 2.into());
        assert_eq!(
            serde_json::to_value(&error).unwrap()["targetId"],
            "target-1"
        );
        assert_eq!(serde_json::to_value(&error).unwrap()["params"]["count"], 2);
    }

    #[test]
    fn native_messages_share_the_frontend_resources() {
        assert_eq!(Locale::ZhHans.messages()["menu.edit"], "编辑");
        assert_eq!(Locale::En.messages()["menu.edit"], "Edit");
    }
}
