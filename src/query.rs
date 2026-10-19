//! What is typed to search, by the TUI's `/` and `paperstack find` alike:
//!
//! ```text
//! perturbation "linear baseline" author:Huber year:2020..2026
//!     folder:"Single Cell Data" -label:benchmark has:pdf is:starred
//! ```
//!
//! Words, and phrases in quotes, are looked for in the title, abstract,
//! authors, venue, keywords, key and identifiers; a word matches the words
//! it starts. `field:` narrows a word to one of those, or filters by what
//! is not text. A `-` before any of them leaves out what it matches.

#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub negated: bool,
    pub field: Field,
    pub value: String,
    /// In quotes: the words together, in order, each whole.
    pub phrase: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    /// Any of the text.
    Any,
    Title,
    Abstract,
    Author,
    Venue,
    Year,
    Folder,
    Label,
    Key,
    Doi,
    /// `has:pdf`, `has:abstract`, `has:doi`.
    Has,
    /// `is:starred`.
    Is,
}

impl Field {
    /// Looked for in the text, rather than held to, as `year:` is.
    pub fn is_text(self) -> bool {
        matches!(
            self,
            Field::Any | Field::Title | Field::Abstract | Field::Author | Field::Venue
        )
    }

    fn named(name: &str) -> Option<Field> {
        Some(match name.to_lowercase().as_str() {
            "title" => Field::Title,
            "abstract" => Field::Abstract,
            "author" | "au" => Field::Author,
            "venue" | "journal" => Field::Venue,
            "year" | "y" => Field::Year,
            "folder" | "in" => Field::Folder,
            "label" | "tag" => Field::Label,
            "key" => Field::Key,
            "doi" => Field::Doi,
            "has" => Field::Has,
            "is" => Field::Is,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    pub terms: Vec<Term>,
}

impl Query {
    pub fn parse(s: &str) -> Query {
        let chars: Vec<char> = s.chars().collect();
        let mut terms = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i].is_whitespace() {
                i += 1;
                continue;
            }
            let negated = chars[i] == '-' && chars.get(i + 1).is_some_and(|c| !c.is_whitespace());
            if negated {
                i += 1;
            }
            // `name:` before the value, if `name` is a field.
            let mut field = Field::Any;
            let mut j = i;
            while j < chars.len() && chars[j].is_alphabetic() {
                j += 1;
            }
            if j > i && chars.get(j) == Some(&':') {
                let name: String = chars[i..j].iter().collect();
                if let Some(f) = Field::named(&name) {
                    field = f;
                    i = j + 1;
                }
            }
            let (value, phrase, next) = if chars.get(i) == Some(&'"') {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&c| c == '"')
                    .map_or(chars.len(), |p| i + 1 + p);
                (chars[i + 1..end].iter().collect::<String>(), true, end + 1)
            } else {
                let end = chars[i..]
                    .iter()
                    .position(|c| c.is_whitespace())
                    .map_or(chars.len(), |p| i + p);
                (chars[i..end].iter().collect::<String>(), false, end)
            };
            i = next;
            let value = value.trim().to_string();
            if !value.is_empty() {
                terms.push(Term {
                    negated,
                    field,
                    value,
                    phrase,
                });
            }
        }
        Query { terms }
    }

    /// `self` and `other` both.
    pub fn and(mut self, other: Query) -> Query {
        self.terms.extend(other.terms);
        self
    }
}

/// `2020`, `2020..2026`, `2020..` or `..2020`, as the years it spans.
pub fn years(s: &str) -> Option<(i32, i32)> {
    let year = |s: &str, none: i32| -> Option<i32> {
        let s = s.trim();
        if s.is_empty() {
            Some(none)
        } else {
            s.parse().ok()
        }
    };
    match s.split_once("..") {
        Some((from, to)) => Some((year(from, i32::MIN)?, year(to, i32::MAX)?)),
        None => {
            let y = s.trim().parse().ok()?;
            Some((y, y))
        }
    }
}
