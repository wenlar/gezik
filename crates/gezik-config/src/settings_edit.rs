//! Edits `settings.toml` in place: only the `pinned` value changes; the user's comments,
//! key order and formatting stay as they were.

/// Returns `text` with `pinned` set to `pinned`. Errors with the parser's message if the
/// file is not valid TOML (the caller must then leave the file alone).
pub fn with_pinned(text: &str, pinned: &[String]) -> Result<String, String> {
    let mut doc = text.parse::<toml_edit::DocumentMut>().map_err(|err| err.to_string().trim().to_owned())?;

    // Check if pinned exists and validate it's an array
    if let Some(existing) = doc.get("pinned")
        && !existing.is_array()
    {
        return Err("pinned must be a list".to_owned());
    }

    // If pinned already exists as an array, clear it and add new elements to preserve decor
    if let Some(item) = doc.get_mut("pinned") {
        if let Some(array) = item.as_array_mut() {
            array.clear();
            for path in pinned {
                array.push(path.as_str());
            }
        }
    } else {
        // Create new pinned array
        let mut array = toml_edit::Array::new();
        for path in pinned {
            array.push(path.as_str());
        }
        doc["pinned"] = toml_edit::value(array);
    }

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keeps_comments_and_other_keys() {
        let text = "# my settings\ntheme = \"nord\" # favourite\npinned = [\"/a\"]\n\n[layout]\n# keep me\nsidebar = \"right\"\n";
        let out = with_pinned(text, &pins(&["/a", "/b"])).unwrap();
        assert!(out.contains("# my settings"));
        assert!(out.contains("theme = \"nord\" # favourite"));
        assert!(out.contains("# keep me"));
        assert!(out.contains("sidebar = \"right\""));
        let parsed = out.parse::<toml::Table>().unwrap();
        let pinned: Vec<_> = parsed["pinned"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(pinned, ["/a", "/b"]);
    }

    #[test]
    fn adds_pinned_as_a_top_level_key_when_missing() {
        let text = "theme = \"auto\"\n\n[layout]\ndensity = \"compact\"\n";
        let out = with_pinned(text, &pins(&["{home}/x"])).unwrap();
        let parsed = out.parse::<toml::Table>().unwrap();
        assert_eq!(parsed["pinned"].as_array().unwrap()[0].as_str(), Some("{home}/x"));
        assert_eq!(parsed["layout"]["density"].as_str(), Some("compact"));
    }

    #[test]
    fn empty_list_is_written() {
        let out = with_pinned("pinned = [\"/a\"]\n", &[]).unwrap();
        assert!(out.parse::<toml::Table>().unwrap()["pinned"].as_array().unwrap().is_empty());
    }

    #[test]
    fn invalid_toml_is_an_error() {
        assert!(with_pinned("theme = \n[layout", &pins(&["/a"])).is_err());
    }

    #[test]
    fn keeps_inline_comment_on_pinned_line() {
        let text = "pinned = [\"/a\"] # my note\n";
        let out = with_pinned(text, &pins(&["/b"])).unwrap();
        assert!(out.contains("# my note"), "inline comment lost");
        let parsed = out.parse::<toml::Table>().unwrap();
        let pinned: Vec<_> = parsed["pinned"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(pinned, ["/b"]);
    }

    #[test]
    fn pinned_appears_before_sections() {
        let text = "theme = \"auto\"\n\n[layout]\ndensity = \"compact\"\n";
        let out = with_pinned(text, &pins(&["{home}/x"])).unwrap();
        let pinned_pos = out.find("pinned").expect("pinned not found");
        let layout_pos = out.find("[layout]").expect("[layout] not found");
        assert!(pinned_pos < layout_pos, "pinned should appear before [layout]");
    }

    #[test]
    fn rejects_pinned_table() {
        let text = "[pinned]\nfolder = \"/a\"\n";
        let err = with_pinned(text, &pins(&["/b"])).unwrap_err();
        assert_eq!(err, "pinned must be a list");
    }

    #[test]
    fn rejects_pinned_string() {
        let text = "pinned = \"/a\"\n";
        let err = with_pinned(text, &pins(&["/b"])).unwrap_err();
        assert_eq!(err, "pinned must be a list");
    }
}
