use std::io::Read;

const USER_AGENT: &str = concat!(
    "eParna-Reader/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/SV-stark/eParna)"
);

pub fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else if b == b' ' {
            encoded.push('+');
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

pub fn parse_dictionary_json(body: &str) -> String {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else {
        return "No definition found".into();
    };
    let Some(entries) = json.as_array() else {
        return "No definition found".into();
    };
    let mut defs = Vec::new();
    for entry in entries {
        let Some(meanings) = entry.get("meanings").and_then(|m| m.as_array()) else {
            continue;
        };
        for meaning in meanings {
            let pos = meaning
                .get("partOfSpeech")
                .and_then(|p| p.as_str())
                .unwrap_or("");
            let Some(definitions) = meaning.get("definitions").and_then(|d| d.as_array()) else {
                continue;
            };
            for def in definitions {
                if let Some(text) = def.get("definition").and_then(|t| t.as_str()) {
                    if pos.is_empty() {
                        defs.push(text.to_string());
                    } else {
                        defs.push(format!("({pos}) {text}"));
                    }
                    if defs.len() >= 4 {
                        return defs.join("\n\n");
                    }
                }
            }
        }
    }
    if defs.is_empty() {
        "No definition found".into()
    } else {
        defs.join("\n\n")
    }
}

pub fn fetch_dictionary_definition(word: &str) -> String {
    let encoded = url_encode(word);
    let url = format!("https://api.dictionaryapi.dev/api/v2/entries/en/{encoded}");
    match ureq::get(&url).header("User-Agent", USER_AGENT).call() {
        Ok(resp) => {
            let body = resp.into_body().read_to_string().unwrap_or_default();
            parse_dictionary_json(&body)
        }
        Err(_) => "Lookup failed (offline or word not found)".into(),
    }
}

pub fn fetch_wikipedia_summary(word: &str) -> String {
    let query = url_encode(&word.replace(' ', "_"));
    let url = format!("https://en.wikipedia.org/api/rest_v1/page/summary/{query}");
    match ureq::get(&url).header("User-Agent", USER_AGENT).call() {
        Ok(resp) => {
            let body = resp.into_body().read_to_string().unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body)
                && let Some(extract) = json.get("extract").and_then(|v| v.as_str())
            {
                extract.to_string()
            } else {
                "No summary found on Wikipedia".into()
            }
        }
        Err(_) => "Wikipedia lookup failed (network error or page not found)".into(),
    }
}

pub fn fetch_translation(text: &str) -> String {
    let encoded = url_encode(text);
    let url = format!("https://api.mymemory.translated.net/get?q={encoded}&langpair=autodetect|en");
    match ureq::get(&url).header("User-Agent", USER_AGENT).call() {
        Ok(resp) => {
            let body = resp.into_body().read_to_string().unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(trans) = json
                    .get("responseData")
                    .and_then(|d| d.get("translatedText"))
                    .and_then(|t| t.as_str())
                {
                    format!("Translation (en):\n{trans}")
                } else {
                    "Could not parse translation response".into()
                }
            } else {
                "Invalid translation response".into()
            }
        }
        Err(e) => format!("Translation failed: {e}"),
    }
}

pub fn fetch_opds_feed(url: &str) -> (String, Vec<(String, String)>) {
    match ureq::get(url).header("User-Agent", USER_AGENT).call() {
        Ok(resp) => {
            let body = resp.into_body().read_to_string().unwrap_or_default();
            let feed = if body.trim_start().starts_with('{') {
                ebook_rs::OpdsFeed::parse_json(&body)
            } else {
                ebook_rs::OpdsFeed::parse_atom_xml(&body)
            };
            match feed {
                Ok(f) => {
                    let entries: Vec<(String, String)> = f
                        .entries
                        .iter()
                        .map(|e| {
                            (
                                e.title.clone(),
                                e.download_link(None)
                                    .map(|l| l.href.clone())
                                    .unwrap_or_default(),
                            )
                        })
                        .collect();
                    let count = entries.len();
                    (format!("{count} entries"), entries)
                }
                Err(e) => (format!("Parse error: {e}"), Vec::new()),
            }
        }
        Err(e) => (format!("Fetch error: {e}"), Vec::new()),
    }
}

pub fn download_opds_book(href: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(href)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_body()
        .as_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world!"), "hello+world%21");
        assert_eq!(url_encode("rust-lang_2024"), "rust-lang_2024");
    }

    #[test]
    fn test_parse_dictionary_json_extracts_pos_and_escaped_quotes() {
        let sample = r#"[{
            "word": "leaf",
            "meanings": [{
                "partOfSpeech": "noun",
                "definitions": [{
                    "definition": "A \"flat\" green blade of a plant."
                }]
            }]
        }]"#;
        let parsed = parse_dictionary_json(sample);
        assert_eq!(parsed, "(noun) A \"flat\" green blade of a plant.");
    }

    #[test]
    fn test_parse_dictionary_json_handles_missing() {
        assert_eq!(parse_dictionary_json("{}"), "No definition found");
        assert_eq!(parse_dictionary_json("invalid"), "No definition found");
    }
}
