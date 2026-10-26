use flate2::read::GzDecoder;
use std::io::Read;
use std::path::Path;

/// Pure-Rust StarDict reader (MIT-licensed).
/// Format specification:
///   .ifo: metadata (key=value lines, e.g. idxoffsetbits=64)
///   .idx: sorted word index with null-terminated string + big-endian offset + size
///   .dict / .dict.dz: raw or gzipped dictionary definitions
pub struct StarDict {
    pub entries: Vec<(String, u64, u32)>, // (word, offset in dict, size)
    pub lowercase_indices: Vec<(String, usize)>, // (lowercase_word, entry_index), sorted for O(log n) case-insensitive search
    pub dict: Vec<u8>,
}

impl StarDict {
    pub fn load(ifo_path: &Path) -> Result<Self, String> {
        let dir = ifo_path.parent().unwrap_or(Path::new("."));
        let stem = ifo_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| "bad ifo filename".to_string())?
            .to_string();

        // Parse .ifo for metadata
        let ifo_content = std::fs::read_to_string(ifo_path).map_err(|e| e.to_string())?;
        let mut idx_offset_64 = false;
        for line in ifo_content.lines() {
            if let Some((k, v)) = line.split_once('=')
                && k.trim() == "idxoffsetbits"
                && v.trim() == "64"
            {
                idx_offset_64 = true;
            }
        }

        // Read .idx
        let idx_path = dir.join(format!("{}.idx", stem));
        let idx_bytes = std::fs::read(&idx_path).map_err(|e| e.to_string())?;
        let mut entries: Vec<(String, u64, u32)> = Vec::new();
        let mut i = 0usize;

        let entry_num_bytes = if idx_offset_64 { 12 } else { 8 };

        while i < idx_bytes.len() {
            let start = i;
            while i < idx_bytes.len() && idx_bytes[i] != 0 {
                i += 1;
            }
            if i >= idx_bytes.len() {
                break;
            }
            let word = String::from_utf8_lossy(&idx_bytes[start..i]).to_string();
            i += 1; // skip null terminator

            if i + entry_num_bytes > idx_bytes.len() {
                break;
            }

            let (offset, size) = if idx_offset_64 {
                let off = u64::from_be_bytes([
                    idx_bytes[i],
                    idx_bytes[i + 1],
                    idx_bytes[i + 2],
                    idx_bytes[i + 3],
                    idx_bytes[i + 4],
                    idx_bytes[i + 5],
                    idx_bytes[i + 6],
                    idx_bytes[i + 7],
                ]);
                let sz = u32::from_be_bytes([
                    idx_bytes[i + 8],
                    idx_bytes[i + 9],
                    idx_bytes[i + 10],
                    idx_bytes[i + 11],
                ]);
                (off, sz)
            } else {
                let off = u32::from_be_bytes([
                    idx_bytes[i],
                    idx_bytes[i + 1],
                    idx_bytes[i + 2],
                    idx_bytes[i + 3],
                ]) as u64;
                let sz = u32::from_be_bytes([
                    idx_bytes[i + 4],
                    idx_bytes[i + 5],
                    idx_bytes[i + 6],
                    idx_bytes[i + 7],
                ]);
                (off, sz)
            };

            i += entry_num_bytes;
            entries.push((word, offset, size));
        }

        // Read .dict (or .dict.dz)
        let dz_path = dir.join(format!("{}.dict.dz", stem));
        let dict_path = dir.join(format!("{}.dict", stem));
        let dict = if dz_path.exists() {
            let f = std::fs::File::open(&dz_path).map_err(|e| e.to_string())?;
            let mut dec = GzDecoder::new(f);
            let mut buf = Vec::new();
            dec.read_to_end(&mut buf).map_err(|e| e.to_string())?;
            buf
        } else {
            std::fs::read(&dict_path).map_err(|e| e.to_string())?
        };

        // Build a sorted lowercase index for correct O(log n) case-insensitive search
        let mut lowercase_indices: Vec<(String, usize)> = entries
            .iter()
            .enumerate()
            .map(|(idx, (word, _, _))| (word.to_lowercase(), idx))
            .collect();
        lowercase_indices.sort_by(|a, b| a.0.cmp(&b.0));

        Ok(StarDict {
            entries,
            lowercase_indices,
            dict,
        })
    }

    pub fn lookup(&self, word: &str) -> Option<String> {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 1. Try exact match first
        let entry_idx = self
            .entries
            .binary_search_by(|e| e.0.as_str().cmp(trimmed))
            .ok()
            .or_else(|| {
                // 2. Case-insensitive binary search in sorted lowercase index
                let lower = trimmed.to_lowercase();
                self.lowercase_indices
                    .binary_search_by(|(lw, _)| lw.as_str().cmp(lower.as_str()))
                    .ok()
                    .map(|pos| self.lowercase_indices[pos].1)
            })?;

        let &(_, offset, size) = self.entries.get(entry_idx)?;
        let start = offset as usize;
        let end = start.saturating_add(size as usize);
        if end > self.dict.len() || start > self.dict.len() {
            return None;
        }

        let raw = &self.dict[start..end];
        let text = String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string();

        Some(format_definition(&text))
    }
}

/// Convert HTML markup in dictionary definitions to readable text with proper
/// linebreaks and decoded HTML entities.
fn format_definition(raw: &str) -> String {
    // Quick replacements for common structural tags
    let preprocessed = raw
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n\n")
        .replace("</div>", "\n")
        .replace("<li>", "\n• ");

    let mut out = String::with_capacity(preprocessed.len());
    let mut in_tag = false;

    for c in preprocessed.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }

    // Decode standard entities
    let decoded = out
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        raw.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_definition() {
        let raw = "<p>Definition 1</p><br/><li>Item &amp; &lt;sub&gt;</li>";
        let formatted = format_definition(raw);
        assert!(formatted.contains("Definition 1"));
        assert!(formatted.contains("Item & <sub"));
        assert!(formatted.contains("•"));
    }

    #[test]
    fn test_stardict_case_insensitive_lookup() {
        let entries = vec![
            ("Apple".to_string(), 0, 5),
            ("banana".to_string(), 5, 6),
            ("Zebra".to_string(), 11, 5),
        ];
        let mut lowercase_indices: Vec<(String, usize)> = entries
            .iter()
            .enumerate()
            .map(|(idx, (word, _, _))| (word.to_lowercase(), idx))
            .collect();
        lowercase_indices.sort_by(|a, b| a.0.cmp(&b.0));

        let dict = b"fruityellowstripes".to_vec();
        let sd = StarDict {
            entries,
            lowercase_indices,
            dict,
        };

        // Exact match
        assert_eq!(sd.lookup("Apple").as_deref(), Some("fruit"));
        // Case-insensitive match
        assert_eq!(sd.lookup("apple").as_deref(), Some("fruit"));
        assert_eq!(sd.lookup("BANANA").as_deref(), Some("yellow"));
        assert_eq!(sd.lookup("zebra").as_deref(), Some("strip"));
        assert_eq!(sd.lookup("nonexistent"), None);
    }
}
