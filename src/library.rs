use crate::config::covers_dir;
use crate::models::RecentBook;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "epub", "mobi", "azw3", "fb2", "pdf", "txt", "md", "docx", "odt", "rtf", "cbz", "lit", "kfx",
    "kepub",
];

pub fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn mime_ext(mime: &str) -> &'static str {
    if mime.contains("jpeg") || mime.contains("jpg") {
        "jpg"
    } else if mime.contains("png") {
        "png"
    } else if mime.contains("gif") {
        "gif"
    } else if mime.contains("webp") {
        "webp"
    } else if mime.contains("bmp") {
        "bmp"
    } else {
        "img"
    }
}

pub fn cache_cover_to_disk(book: &ebook_rs::Book, path: &str) -> String {
    let Some((bytes, mime)) = book.cover_image() else {
        return String::new();
    };
    if bytes.len() > 256 * 1024 {
        return String::new();
    }
    let dir = covers_dir();
    let file = dir.join(format!("{}.{}", simple_hash(path), mime_ext(mime)));
    std::fs::write(&file, &bytes)
        .ok()
        .map(|_| file.to_string_lossy().to_string())
        .unwrap_or_default()
}

pub fn scan_folder_books(
    folder: PathBuf,
    existing_paths: HashSet<String>,
    mut existing_hashes: HashSet<String>,
) -> Vec<RecentBook> {
    let mut found = Vec::new();
    let mut stack = vec![folder];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.is_file()
                    && let Some(ext) = p.extension().and_then(|e| e.to_str())
                    && SUPPORTED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
                {
                    found.push(p);
                }
            }
        }
    }

    let now = format!(
        "{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    let mut scanned = Vec::new();
    for p in found {
        let key = p.to_string_lossy().to_string();
        if existing_paths.contains(&key) {
            continue;
        }
        if let Ok(book) = ebook_rs::Book::from_file(&p) {
            let file_size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let content_hash = if file_size <= 32 * 1024 * 1024 {
                book.fingerprint().content_hash
            } else {
                String::new()
            };
            if !content_hash.is_empty() && !existing_hashes.insert(content_hash.clone()) {
                continue;
            }
            let title = book.metadata().title.clone();
            let author = book.metadata().creator().to_string();
            let format = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let total = book.sections().len();
            let cover = cache_cover_to_disk(&book, &key);

            scanned.push(RecentBook {
                path: key,
                title,
                section: 0,
                opened: now.clone(),
                favorite: false,
                status: "reading".into(),
                content_hash,
                cfi: String::new(),
                author,
                format,
                total,
                cover,
                reading_time_secs: 0,
            });
        }
    }
    scanned
}

pub fn merge_scanned_books(recent: &mut Vec<RecentBook>, scanned: Vec<RecentBook>) -> usize {
    let mut added = 0usize;
    for book in scanned {
        let dup_path = recent.iter().any(|r| r.path == book.path);
        let dup_hash = !book.content_hash.is_empty()
            && recent
                .iter()
                .any(|r| !r.content_hash.is_empty() && r.content_hash == book.content_hash);
        if dup_path || dup_hash {
            continue;
        }
        recent.push(book);
        added += 1;
    }
    added
}

pub fn book_pct(r: &RecentBook) -> f32 {
    if r.total == 0 {
        0.0
    } else {
        ((r.section + 1) as f32 / r.total as f32 * 100.0).round()
    }
}

pub fn book_title(r: &RecentBook) -> String {
    if r.title.is_empty() {
        r.path.clone()
    } else {
        r.title.clone()
    }
}

pub fn book_subtitle(r: &RecentBook) -> String {
    match (r.author.is_empty(), r.format.is_empty()) {
        (false, false) => format!("{} \u{b7} {}", r.author, r.format),
        (false, true) => r.author.clone(),
        (true, false) => r.format.clone(),
        (true, true) => String::new(),
    }
}

pub fn star_label(favorite: bool) -> &'static str {
    if favorite { "\u{2605}" } else { "\u{2606}" }
}

pub fn cover_for(covers: &HashMap<String, String>, r: &RecentBook) -> Option<String> {
    if r.cover.is_empty() {
        None
    } else {
        covers.get(&r.cover).cloned()
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_scanned_books_deduplicates_by_path_and_hash() {
        let mut recent = vec![RecentBook {
            path: "/a/book1.epub".into(),
            content_hash: "hash1".into(),
            ..Default::default()
        }];

        let scanned = vec![
            RecentBook {
                path: "/a/book1.epub".into(),
                content_hash: "hash1".into(),
                ..Default::default()
            },
            RecentBook {
                path: "/b/copy_of_book1.epub".into(),
                content_hash: "hash1".into(),
                ..Default::default()
            },
            RecentBook {
                path: "/b/book2.epub".into(),
                content_hash: "hash2".into(),
                ..Default::default()
            },
        ];

        let added = merge_scanned_books(&mut recent, scanned);
        assert_eq!(added, 1);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[1].path, "/b/book2.epub");
    }

    #[test]
    fn test_book_pct_and_truncate() {
        let r = RecentBook {
            section: 4,
            total: 10,
            ..Default::default()
        };
        assert_eq!(book_pct(&r), 50.0);
        assert_eq!(book_pct(&RecentBook::default()), 0.0);

        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hell\u{2026}");
    }

    #[test]
    fn test_simple_hash_is_deterministic() {
        assert_eq!(simple_hash("eparna"), simple_hash("eparna"));
        assert_ne!(simple_hash("eparna"), simple_hash("epustak"));
    }
}
