//! A paper's record, the `.json` file beside its PDF: the paper as CSL JSON,
//! as doi.org gives it and pandoc and ohp read it, and what paperstack keeps
//! of it besides.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    /// The citation key, Paperpile's `Surname2025-xy`: given once, never
    /// changed, and the start of the files' names.
    pub key: String,
    pub csl: Value,
    #[serde(default)]
    pub paperstack: Meta,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Meta {
    /// When it was added, RFC 3339; newest first sorts by it.
    #[serde(default)]
    pub added: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub starred: bool,
    /// CSL has no field of its own for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arxiv: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_sha256: Option<String>,
    /// Keys of papers merged into this one, which still cite it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Where the metadata came from, as `doi.org` or `paperpile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// A person's name, or an organisation's, as `family` alone.
#[derive(Clone, Debug, PartialEq)]
pub struct Name {
    pub family: String,
    pub given: String,
}

impl Record {
    fn text(&self, field: &str) -> Option<&str> {
        match self.csl.get(field)? {
            Value::String(s) => Some(s.trim()).filter(|s| !s.is_empty()),
            // Crossref's API gives some fields as lists.
            Value::Array(a) => a.first()?.as_str().map(str::trim),
            _ => None,
        }
    }

    pub fn title(&self) -> &str {
        self.text("title").unwrap_or("(untitled)")
    }

    pub fn venue(&self) -> &str {
        self.text("container-title")
            .or_else(|| self.text("publisher"))
            .unwrap_or("")
    }

    pub fn doi(&self) -> Option<&str> {
        self.text("DOI")
    }

    pub fn abstract_text(&self) -> Option<&str> {
        self.text("abstract")
    }

    pub fn kind(&self) -> &str {
        match self.text("type").unwrap_or("") {
            "article-journal" | "article" => "Journal article",
            "paper-conference" => "Conference paper",
            "book" => "Book",
            "chapter" => "Book chapter",
            "thesis" => "Thesis",
            "report" => "Report",
            "posted-content" | "manuscript" => "Preprint",
            "dataset" => "Dataset",
            "software" => "Software",
            "webpage" | "post" | "post-weblog" => "Web page",
            "" => "Item",
            other => other,
        }
    }

    pub fn authors(&self) -> Vec<Name> {
        let Some(names) = self.csl.get("author").and_then(Value::as_array) else {
            return Vec::new();
        };
        names
            .iter()
            .filter_map(|n| {
                let part = |k| n.get(k).and_then(Value::as_str).map(str::trim);
                if let Some(literal) = part("literal") {
                    return Some(Name {
                        family: literal.to_string(),
                        given: String::new(),
                    });
                }
                let family = [part("non-dropping-particle"), part("family")]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                let given = part("given").unwrap_or("").to_string();
                (!family.is_empty() || !given.is_empty()).then_some(Name { family, given })
            })
            .collect()
    }

    /// The year issued, from any of CSL's forms of a date.
    pub fn year(&self) -> Option<i32> {
        let issued = self.csl.get("issued")?;
        if let Some(y) = issued
            .get("date-parts")
            .and_then(|d| d.get(0)?.get(0))
            .and_then(|y| y.as_i64().or_else(|| y.as_str()?.trim().parse().ok()))
        {
            return i32::try_from(y).ok();
        }
        let raw = issued
            .get("raw")
            .or_else(|| issued.get("literal"))
            .and_then(Value::as_str)
            .or_else(|| issued.as_str())?;
        raw.split(|c: char| !c.is_ascii_digit())
            .find(|w| w.len() == 4)?
            .parse()
            .ok()
    }

    /// The paper as a CSL item, its `id` the key.
    pub fn item(&self) -> Value {
        let mut item = self.csl.clone();
        if let Value::Object(map) = &mut item {
            map.insert("id".to_string(), Value::String(self.key.clone()));
        }
        item
    }
}

/// A key in Paperpile's form, `Surname2025-xy`: the first author's family
/// name in ASCII, the year, and two letters from `identity`, a DOI or a
/// title, so a paper gets the same key wherever it is added. `taken` says
/// which keys are another paper's; the letters are drawn again until one is
/// free.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "`paperstack add` gives keys with it")
)]
pub fn new_key(record: &Record, identity: &str, taken: impl Fn(&str) -> bool) -> String {
    let family = record
        .authors()
        .first()
        .map_or_else(|| "Anon".to_string(), |n| n.family.clone());
    let surname: String = deunicode::deunicode(&family)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let surname = if surname.is_empty() {
        "Anon".to_string()
    } else {
        surname
    };
    let year = record
        .year()
        .map_or_else(|| "nd".to_string(), |y| y.to_string());
    let identity = identity.trim().to_lowercase();
    for salt in 0u64.. {
        let h = fnv(identity.as_bytes(), salt);
        let letters: String = [h % 26, (h / 26) % 26]
            .iter()
            .map(|&n| char::from(b'a' + n as u8))
            .collect();
        let key = format!("{surname}{year}-{letters}");
        if !taken(&key) {
            return key;
        }
    }
    unreachable!("some letters are free")
}

/// FNV-1a, seeded by `salt`: stable across builds and machines, as std's
/// hasher is not.
pub fn fnv(bytes: &[u8], salt: u64) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64 ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
