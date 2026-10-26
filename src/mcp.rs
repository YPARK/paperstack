//! paperstack as an MCP server, for an AI agent to search the library,
//! read its papers, cite them, add to it and delete from it: `paperstack mcp`, run by the
//! agent's program, as Cursor or Claude Code, which speaks to it in
//! JSON-RPC, a message a line, on its stdin and stdout. Nothing else is
//! written to stdout; what goes wrong is said on stderr.
//!
//! The index is brought up to the library at each call, so what the TUI,
//! `paperstack serve` or another machine added meanwhile is found.

use crate::add::{Adder, Options, attach, one_line, open_copies};
use crate::bibtex;
use crate::index::{Chosen, Hit, Index, Picked, Sort};
use crate::library::Library;
use crate::net::Net;
use crate::query::Query;
use anyhow::{Context, bail};
use regex::Regex;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::sync::LazyLock;

/// The protocol's version answered when the client's is not one known.
const PROTOCOL: &str = "2025-06-18";
const KNOWN: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
/// The most pages `read_paper` gives at once, and characters of them.
const PAGES_AT_ONCE: u32 = 20;
const TEXT_LIMIT: usize = 200_000;
/// Authors listed of a paper found, before their count stands for the rest.
const AUTHORS_SHOWN: usize = 10;
/// Where a tool's result is text, as a bibliography, with data about it:
/// the text, given first, and the data after it.
const TEXT: &str = "__text";

pub struct Server {
    pub lib: Library,
    pub index: Index,
    pub net: Net,
}

impl Server {
    /// Answer messages on stdin until it closes.
    pub fn run(&mut self) -> anyhow::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(Value::Array(batch)) => {
                    let replies: Vec<Value> = batch.iter().filter_map(|m| self.answer(m)).collect();
                    (!replies.is_empty()).then(|| Value::Array(replies))
                }
                Ok(message) => self.answer(&message),
                Err(e) => Some(error(Value::Null, -32700, &format!("not JSON: {e}"))),
            };
            if let Some(reply) = reply {
                writeln!(stdout, "{reply}")?;
                stdout.flush()?;
            }
        }
        let _ = self.index.flush();
        Ok(())
    }

    /// The reply to `message`; none to a notification.
    pub fn answer(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let id = id?;
        let result = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => Ok(self.call(&params)),
            "resources/list" => Ok(json!({ "resources": [{
                "uri": "paperstack://agents",
                "name": "How to use this library",
                "description": "The library's AGENTS.md: how papers are filed, found and cited",
                "mimeType": "text/markdown"
            }] })),
            "resources/read" => Ok(json!({ "contents": [{
                "uri": "paperstack://agents",
                "mimeType": "text/markdown",
                "text": crate::agents::TEXT
            }] })),
            "prompts/list" => Ok(json!({ "prompts": [] })),
            _ => Err((-32601, format!("no method {method}"))),
        };
        Some(match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, why)) => error(id, code, &why),
        })
    }

    /// A tool's result: its text, and what it found as data; a tool that
    /// fails says why, as its result, for the agent to read.
    fn call(&mut self, params: &Value) -> Value {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let done = self
            .index
            .refresh(&self.lib)
            .and_then(|_| self.tool(name, &args));
        let _ = self.index.flush();
        match done {
            Ok(mut data) => {
                let given = data.as_object_mut().and_then(|m| m.remove(TEXT));
                let pretty = |v: &Value| serde_json::to_string_pretty(v).unwrap_or_default();
                let content = match (&data, given) {
                    (Value::String(s), _) => vec![json!({ "type": "text", "text": s })],
                    (other, Some(text)) => vec![
                        json!({ "type": "text", "text": text }),
                        json!({ "type": "text", "text": pretty(other) }),
                    ],
                    (other, None) => vec![json!({ "type": "text", "text": pretty(other) })],
                };
                let mut result = json!({ "content": content, "isError": false });
                if data.is_object() {
                    result["structuredContent"] = data;
                }
                result
            }
            Err(e) => json!({
                "content": [{ "type": "text", "text": one_line(&e) }],
                "isError": true
            }),
        }
    }

    fn tool(&mut self, name: &str, args: &Value) -> anyhow::Result<Value> {
        let text = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string);
        let list = |k: &str| -> Vec<String> {
            args.get(k)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        };
        match name {
            "search_papers" => {
                let query = text("query").unwrap_or_default();
                let sort = match text("sort").as_deref() {
                    Some("year") => Sort::Year,
                    Some("added") => Sort::Added,
                    Some("author") => Sort::Author,
                    Some("title") => Sort::Title,
                    _ => Sort::Relevance,
                };
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
                let hits = self
                    .index
                    .search(&Query::parse(&query), sort, false, None)?;
                let total = hits.len();
                let papers: Vec<Value> = hits
                    .iter()
                    .take(limit)
                    .map(|h| self.paper(h, false))
                    .collect();
                Ok(json!({ "total": total, "shown": papers.len(), "papers": papers }))
            }
            "get_paper" => {
                let hit = self.hit(&text("key").context("give the paper's key")?)?;
                match text("format").as_deref() {
                    Some("bibtex") => Ok(Value::String(bibtex::entry(&hit.record))),
                    Some("csl") => Ok(hit.record.item()),
                    _ => Ok(self.paper(&hit, true)),
                }
            }
            "cite" => {
                if let Some(path) = text("output") {
                    let output = std::path::PathBuf::from(path);
                    let asked = match text("format").as_deref() {
                        Some("keys") => bail!("[@a; @b] citations are not written to a file"),
                        Some("csl") => Some(crate::kept::Format::Csl),
                        Some(_) => Some(crate::kept::Format::Bibtex),
                        None => None,
                    };
                    let format = crate::kept::Format::asked(&output, asked)?;
                    let bib = crate::kept::Bib {
                        output,
                        query: text("query"),
                        keys: text("key").into_iter().chain(list("keys")).collect(),
                        format,
                    };
                    let keep = args.get("keep").and_then(Value::as_bool).unwrap_or(false);
                    let output = std::path::absolute(&bib.output)?;
                    let papers = crate::kept::write_out(&self.index, bib, keep)?;
                    return Ok(json!({
                        "entries": papers.len(),
                        "merged": merged(&papers),
                        "output": output,
                        "kept": keep,
                    }));
                }
                let Chosen { papers, missing } = self.chosen(args)?;
                if papers.is_empty() && missing.is_empty() {
                    bail!("give keys, or a query, of the papers to cite");
                }
                let out = match text("format").as_deref() {
                    Some("keys") => format!(
                        "[{}]",
                        papers
                            .iter()
                            .map(|p| format!("@{}", p.asked))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    Some("csl") => crate::kept::render(&papers, crate::kept::Format::Csl)?,
                    _ => crate::kept::render(&papers, crate::kept::Format::Bibtex)?,
                };
                Ok(json!({
                    "entries": papers.len(),
                    "missing": missing,
                    "merged": merged(&papers),
                    TEXT: out,
                }))
            }
            "refresh_papers" => {
                let Chosen { papers, missing } = self.chosen(args)?;
                if papers.is_empty() && missing.is_empty() {
                    bail!("give keys, or a query, of the papers to read again");
                }
                let dry_run = args
                    .get("dry_run")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let mut done = Vec::new();
                for p in papers {
                    let key = p.hit.record.key.clone();
                    done.push(
                        match crate::add::refresh(
                            &self.lib,
                            &mut self.index,
                            &self.net,
                            &p.hit,
                            dry_run,
                        ) {
                            Ok(changed) => json!({
                                "key": key,
                                "changed": changed.iter().map(|c| &c.field).collect::<Vec<_>>(),
                                "changes": changed.iter().map(|c| c.said()).collect::<Vec<_>>(),
                            }),
                            Err(e) => json!({ "key": key, "error": one_line(&e) }),
                        },
                    );
                }
                Ok(json!({ "papers": done, "missing": missing, "dry_run": dry_run }))
            }
            "restore_papers" => {
                let keys = list("keys");
                if keys.is_empty() {
                    bail!("give the keys of the papers to bring back from the trash");
                }
                let pdf = args.get("pdf").and_then(Value::as_bool).unwrap_or(false);
                let mut done = Vec::new();
                for key in &keys {
                    let back = if pdf {
                        crate::remove::restore_pdf(&self.lib, &mut self.index, key)
                    } else {
                        crate::remove::restore(&self.lib, &mut self.index, key)
                    };
                    done.push(match back {
                        Ok(path) => json!({ "key": key, "path": self.lib.root.join(path) }),
                        Err(e) => json!({ "key": key, "error": one_line(&e) }),
                    });
                }
                Ok(json!({ "restored": done }))
            }
            "check_keys" => {
                if !args.get("keys").is_some_and(Value::is_array) && text("path").is_none() {
                    bail!(
                        "give the keys cited, as a list, or the path of the text that cites them"
                    );
                }
                let mut cited = list("keys");
                if let Some(path) = text("path") {
                    let text = std::fs::read_to_string(&path)
                        .with_context(|| format!("cannot read {path}"))?;
                    cited.extend(cited_keys(&text));
                }
                let Chosen { papers, missing } =
                    self.index.choose(cited.iter().map(String::as_str), None)?;
                let found: Vec<&str> = papers
                    .iter()
                    .filter(|p| p.asked == p.hit.record.key)
                    .map(|p| p.asked.as_str())
                    .collect();
                Ok(json!({ "found": found, "merged": merged(&papers), "missing": missing }))
            }
            "read_paper" => {
                let hit = self.hit(&text("key").context("give the paper's key")?)?;
                if !hit.has_pdf {
                    bail!(
                        "{} has no PDF here: its abstract is in get_paper",
                        hit.record.key
                    );
                }
                let path = self.lib.root.join(&hit.path).with_extension("pdf");
                let bytes = std::fs::read(&path)?;
                let first = args
                    .get("first_page")
                    .and_then(Value::as_u64)
                    .unwrap_or(1)
                    .max(1) as u32;
                let count = args
                    .get("pages")
                    .and_then(Value::as_u64)
                    .unwrap_or(PAGES_AT_ONCE as u64)
                    .clamp(1, PAGES_AT_ONCE as u64) as u32;
                let (pages, total) = crate::pdf::pages(&bytes, first, count)?;
                let mut text: String = pages
                    .iter()
                    .enumerate()
                    .map(|(i, p)| format!("--- page {} ---\n{}\n", first + i as u32, p.trim()))
                    .collect();
                if text.len() > TEXT_LIMIT {
                    let mut at = TEXT_LIMIT;
                    while !text.is_char_boundary(at) {
                        at -= 1;
                    }
                    text.truncate(at);
                    text.push_str("\n[cut short]");
                }
                let last = first + pages.len() as u32 - 1;
                let more = if last < total {
                    format!(
                        "\n[pages {first}–{last} of {total}; first_page {} for more]",
                        last + 1
                    )
                } else {
                    format!("\n[pages {first}–{last} of {total}]")
                };
                Ok(Value::String(format!(
                    "{}\n\n{text}{more}",
                    hit.record.title()
                )))
            }
            "list_collections" => {
                let s = self.index.shelves()?;
                Ok(json!({
                    "papers": s.all,
                    "starred": s.starred,
                    "without_pdf": s.no_pdf,
                    "collections": s.folders.iter().map(|(n, c)| json!({ "name": n, "papers": c })).collect::<Vec<_>>(),
                    "labels": s.labels.iter().map(|(n, c)| json!({ "name": n, "papers": c })).collect::<Vec<_>>(),
                }))
            }
            "add_paper" => {
                let what = text("what")
                    .context("give a DOI, arXiv id, PMID, address, title or PDF path")?;
                let mut adder = Adder {
                    lib: &self.lib,
                    index: &mut self.index,
                    net: &self.net,
                    options: Options {
                        pdf: args.get("pdf").and_then(Value::as_bool).unwrap_or(true),
                        folders: list("collections"),
                        labels: list("labels"),
                    },
                };
                let added = adder.add(&what)?;
                let hit = self.hit(&added.key)?;
                Ok(json!({ "outcome": added.summary(), "paper": self.paper(&hit, false) }))
            }
            "attach_pdf" => {
                if let Some(path) = text("path") {
                    let key = text("key").context("give the paper's key, with the PDF's path")?;
                    let path = crate::browser::dropped(&path);
                    let bytes = std::fs::read(&path)
                        .with_context(|| format!("cannot read {}", path.display()))?;
                    let added = attach(
                        &self.lib,
                        &mut self.index,
                        &key,
                        &bytes,
                        &path.display().to_string(),
                        false,
                    )?;
                    return Ok(json!({ "outcome": added.summary(), "key": added.key }));
                }
                let published = args
                    .get("published")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let Chosen { papers, missing } = self.chosen(args)?;
                let done = open_copies(
                    &self.lib,
                    &mut self.index,
                    &self.net,
                    papers,
                    published,
                    &|_| true,
                );
                let done: Vec<Value> = done
                    .into_iter()
                    .map(|(key, got)| {
                        let mut v = match got {
                            Ok(a) => crate::cli::added_json(&self.lib, &a, &key),
                            Err(e) => crate::cli::failed_json(&e, &key),
                        };
                        v["key"] = json!(key);
                        v
                    })
                    .collect();
                Ok(json!({ "papers": done, "missing": missing }))
            }
            "update_paper" => {
                let Chosen { papers, missing } = self.chosen(args)?;
                let one = args.get("key").is_some() && args.get("keys").is_none();
                if papers.is_empty() && missing.is_empty() {
                    bail!("give the paper's key, or keys, or a query");
                }
                let aliases = list("add_aliases");
                let set: Vec<(String, Option<Value>)> = args
                    .get("set")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), (!v.is_null()).then(|| v.clone())))
                    .collect();
                let reset = list("reset");
                if (!set.is_empty() || !reset.is_empty()) && papers.len() != 1 {
                    bail!("fields are set on one paper, by its key");
                }
                if !aliases.is_empty() && papers.len() != 1 {
                    bail!("aliases are given to one paper, by its key");
                }
                for alias in &aliases {
                    if let Some(other) = self.index.get(alias)?
                        && other.record.key != papers[0].hit.record.key
                    {
                        bail!("{alias} is {}'s key already", other.record.key);
                    }
                }
                let mut updated = Vec::new();
                for Picked { hit, .. } in &papers {
                    let Some((_, mut record)) = self.index.fresh(&self.lib, &hit.record.key)?
                    else {
                        continue;
                    };
                    let before = record.clone();
                    crate::add::set_fields(&mut record, &set)?;
                    crate::add::reset_fields(&mut record, &reset);
                    let m = &mut record.paperstack;
                    for (have, add, remove) in [
                        (
                            &mut m.folders,
                            list("add_collections"),
                            list("remove_collections"),
                        ),
                        (&mut m.labels, list("add_labels"), list("remove_labels")),
                        (&mut m.aliases, aliases.clone(), Vec::new()),
                    ] {
                        have.retain(|h| !remove.iter().any(|r| r.eq_ignore_ascii_case(h)));
                        for a in add {
                            if !have.iter().any(|h| h.eq_ignore_ascii_case(&a)) {
                                have.push(a);
                            }
                        }
                    }
                    m.aliases.retain(|a| !a.eq_ignore_ascii_case(&record.key));
                    if let Some(star) = args.get("starred").and_then(Value::as_bool) {
                        m.starred = star;
                    }
                    if record == before {
                        continue;
                    }
                    let path = self.lib.save(&record, Some(&hit.path))?;
                    updated.push(record.key.clone());
                    self.index
                        .upsert(&self.lib, &path, record, Some(&hit.path))?;
                }
                if one {
                    let hit = self.hit(&text("key").unwrap_or_default())?;
                    let mut paper = self.paper(&hit, false);
                    if !set.is_empty() {
                        // The fields set, as they are now.
                        let now: serde_json::Map<String, Value> = set
                            .iter()
                            .map(|(f, _)| {
                                (
                                    f.clone(),
                                    hit.record.csl.get(f).cloned().unwrap_or(Value::Null),
                                )
                            })
                            .collect();
                        paper["set"] = Value::Object(now);
                    }
                    return Ok(paper);
                }
                Ok(json!({
                    "updated": updated.len(),
                    "keys": updated,
                    "unchanged": papers.len() - updated.len(),
                    "missing": missing,
                }))
            }
            "delete_papers" => {
                let keys = list("keys");
                if keys.is_empty() {
                    bail!("give the keys of the papers to delete");
                }
                let mut deleted = Vec::new();
                for key in &keys {
                    let trash = crate::remove::paper(&self.lib, &mut self.index, key)?;
                    deleted.push(json!({ "key": key, "trash": self.lib.root.join(trash) }));
                }
                Ok(json!({ "deleted": deleted }))
            }
            "delete_collection" => {
                let name = text("name").context("give the collection's name")?;
                let shelf = match text("kind").as_deref() {
                    Some("label") => crate::remove::Shelf::Label,
                    _ => crate::remove::Shelf::Collection,
                };
                let n = crate::remove::shelf(&self.lib, &mut self.index, shelf, &name)?;
                Ok(json!({ "deleted": name, "kind": shelf.name(), "papers": n }))
            }
            _ => bail!("no tool {name}"),
        }
    }

    fn hit(&self, key: &str) -> anyhow::Result<Hit> {
        self.index
            .get(key)?
            .with_context(|| format!("no paper with the key {key}: search_papers finds keys"))
    }

    /// The papers `args` names: by `key`, `keys`, or a `query`.
    fn chosen(&self, args: &Value) -> anyhow::Result<Chosen> {
        let keys = args
            .get("key")
            .into_iter()
            .chain(
                args.get("keys")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten(),
            )
            .filter_map(Value::as_str);
        self.index
            .choose(keys, args.get("query").and_then(Value::as_str))
    }

    /// A paper as the agent is given it; with its abstract, and all its
    /// authors, if `whole`.
    fn paper(&self, h: &Hit, whole: bool) -> Value {
        let mut v = crate::cli::json(&self.lib, h);
        if let Value::Object(map) = &mut v {
            if let Some(f) = map.remove("folders") {
                map.insert("collections".to_string(), f);
            }
            if !whole
                && let Some(Value::Array(authors)) = map.get_mut("authors")
                && authors.len() > AUTHORS_SHOWN
            {
                let total = authors.len();
                authors.truncate(AUTHORS_SHOWN);
                map.insert("authors_total".to_string(), json!(total));
            }
            if whole {
                map.insert("abstract".to_string(), json!(h.record.abstract_text()));
                map.insert("bibtex".to_string(), json!(bibtex::entry(&h.record)));
            }
        }
        v
    }
}

/// Of the papers asked for, those asked by a key merged into another's,
/// or written otherwise, as pandoc, which tells keys by their case, would
/// not find.
fn merged(papers: &[Picked]) -> Vec<Value> {
    papers
        .iter()
        .filter(|p| p.asked != p.hit.record.key)
        .map(|p| json!({ "key": p.asked, "into": p.hit.record.key }))
        .collect()
}

/// The keys a text cites, in order, once each: pandoc's `@key` and
/// `[@key]`, and LaTeX's `\cite{key,key}`; not an address's `@`, nor
/// a cross-reference, as `\@ref(fig:a)` or `@fig-a`.
pub fn cited_keys(text: &str) -> Vec<String> {
    static PANDOC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:^|[^\w\\@])@(?:\{([^}]+)\}|([\w][\w:.#$%&\-+?<>~/]*))").unwrap()
    });
    static LATEX: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\\[A-Za-z]*cite[A-Za-z]*\*?(?:\[[^\]]*\])*\{([^}]*)\}").unwrap()
    });
    const REFS: &[&str] = &["fig", "tbl", "sec", "eq", "lst", "tab", "thm"];
    let mut keys: Vec<(usize, String)> = Vec::new();
    for c in PANDOC.captures_iter(text) {
        let m = c.get(1).or_else(|| c.get(2)).expect("one or the other");
        let key = m.as_str().trim_end_matches(['.', ':', ',', ';', '?', '!']);
        let is_ref = REFS.iter().any(|r| {
            key.strip_prefix(r)
                .is_some_and(|rest| rest.starts_with([':', '-']))
        });
        if !is_ref && !key.is_empty() {
            keys.push((m.start(), key.to_string()));
        }
    }
    for c in LATEX.captures_iter(text) {
        let m = c.get(1).expect("its keys");
        for key in m
            .as_str()
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
        {
            keys.push((m.start(), key.to_string()));
        }
    }
    keys.sort_by_key(|(at, _)| *at);
    let mut seen = std::collections::HashSet::new();
    keys.into_iter()
        .map(|(_, k)| k)
        .filter(|k| seen.insert(k.clone()))
        .collect()
}

fn initialize(params: &Value) -> Value {
    let asked = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL);
    let version = if KNOWN.contains(&asked) {
        asked
    } else {
        PROTOCOL
    };
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {}, "resources": {}, "prompts": {} },
        "serverInfo": { "name": "paperstack", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "A library of scholarly papers, each with its PDF where there is one. \
            search_papers finds papers by words in their title, abstract and authors, and by fields: \
            author:, year:2020..2026, collection:\"name\", label:, has:pdf, is:starred, and - before any \
            to leave out. Cite only keys these tools give; never make one up. read_paper gives a \
            paper's text, a few pages at a time."
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The tools, as `tools/list` gives them: what each is for, what it takes,
/// and whether it changes the library.
fn tools() -> Value {
    let read_only = json!({ "readOnlyHint": true, "openWorldHint": false });
    json!([
        {
            "name": "search_papers",
            "title": "Search the library",
            "description": "Find papers in the library. Words and \"phrases\" are looked for in titles, abstracts, authors, venues and keys, a word matching the words it starts. Fields: title: abstract: author: venue: year:2020..2026 collection:\"name\" label: key: doi: has:pdf has:abstract is:starred; - before any leaves out what it matches. Empty, it lists every paper. Of a paper with more than 10 authors, the first 10 are listed, and authors_total counts them all; get_paper lists them all.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "e.g. perturbation \"linear baseline\" author:Halvorsen year:2020.." },
                "sort": { "type": "string", "enum": ["relevance", "year", "added", "author", "title"] },
                "limit": { "type": "integer", "minimum": 1, "default": 20 }
            } },
            "annotations": read_only
        },
        {
            "name": "get_paper",
            "title": "Get a paper",
            "description": "One paper by its key: its metadata, abstract and BibTeX; or only its BibTeX, or its CSL JSON.",
            "inputSchema": { "type": "object", "required": ["key"], "properties": {
                "key": { "type": "string" },
                "format": { "type": "string", "enum": ["json", "bibtex", "csl"], "default": "json" }
            } },
            "annotations": read_only
        },
        {
            "name": "cite",
            "title": "Make a bibliography",
            "description": "BibTeX, CSL JSON, or [@key; @key] citations, of the papers with the keys given, or those a query finds, or both; or written to a file, output, a .bib (or .json for csl), only when every key given is had; with keep, kept up to date whenever the library changes, never cut short where a key goes missing. A key merged into another paper's is cited under the key given. Keys no paper has are listed as missing, the rest cited. BibTeX titles keep their case through citeproc: capitals the authors gave are braced.",
            "inputSchema": { "type": "object", "properties": {
                "keys": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string" },
                "format": { "type": "string", "enum": ["bibtex", "csl", "keys"], "default": "bibtex" },
                "output": { "type": "string", "description": "a .bib or .json file to write, replaced whole" },
                "keep": { "type": "boolean", "default": false, "description": "with output: keep the file up to date as the library changes, from this machine's paperstack" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "check_keys",
            "title": "Check cited keys",
            "description": "Which of the keys a manuscript cites the library has: found; merged, an older key of a paper now under another, which cite still gives under the old key; or missing. Give the keys, or the path of the text citing them (pandoc's @key, LaTeX's \\cite{key}).",
            "inputSchema": { "type": "object", "properties": {
                "keys": { "type": "array", "items": { "type": "string" } },
                "path": { "type": "string", "description": "a .rmd, .md, .qmd or .tex file" }
            } },
            "annotations": read_only
        },
        {
            "name": "read_paper",
            "title": "Read a paper",
            "description": "The text of a paper's PDF, up to 20 pages at a time, page by page. Say first_page for more.",
            "inputSchema": { "type": "object", "required": ["key"], "properties": {
                "key": { "type": "string" },
                "first_page": { "type": "integer", "minimum": 1, "default": 1 },
                "pages": { "type": "integer", "minimum": 1, "maximum": 20, "default": 20 }
            } },
            "annotations": read_only
        },
        {
            "name": "list_collections",
            "title": "List collections",
            "description": "The library's collections and labels, with how many papers each holds.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": read_only
        },
        {
            "name": "add_paper",
            "title": "Add a paper",
            "description": "Add a paper by its DOI, arXiv id, PMID, a publisher's address, a PDF's path, or its exact title, its record read from Crossref, arXiv or PubMed and an open copy of its PDF downloaded; no browser is opened. A paper already here is not added again. A title several papers have adds none, and lists them, to add by id.",
            "inputSchema": { "type": "object", "required": ["what"], "properties": {
                "what": { "type": "string", "description": "e.g. 10.5555/demo.2026.01, arXiv:2412.99901, PMID: 99999901" },
                "collections": { "type": "array", "items": { "type": "string" } },
                "labels": { "type": "array", "items": { "type": "string" } },
                "pdf": { "type": "boolean", "default": true, "description": "download an open copy of the PDF" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        },
        {
            "name": "attach_pdf",
            "title": "Give a paper its PDF",
            "description": "Give papers without one their PDF. Without a path, open copies are downloaded, published versions first, for each paper of the keys or the query (those it finds without a PDF); no browser is opened. Each paper's outcome is listed: its pdf_version, or why none was had (blocked, where a copy was turned away, as by a paywall or a bot check, so the person may get it in the browser at its page; or no-open-copy), with each copy tried and its reason (http-429, http-403, html-page, not-a-pdf, not-the-paper, preprint, network). published: no preprint, and a paper whose PDF is a preprint's is given the published version in its place. With a path, one paper is given the PDF there, as one downloaded in a browser: only if its first page has the paper's title, or, for a scan with too little text, the paper's DOI. A scan without it is refused: the person may attach it themselves, by `paperstack attach KEY FILE`.",
            "inputSchema": { "type": "object", "properties": {
                "key": { "type": "string" },
                "keys": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string", "description": "e.g. collection:\"name\"" },
                "path": { "type": "string", "description": "a PDF on this machine, for the paper of key" },
                "published": { "type": "boolean", "default": false }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        },
        {
            "name": "update_paper",
            "title": "Change papers' collections, labels or star",
            "description": "Put papers in collections or take them out, add or take away labels, star them or not: one paper by its key, or many, by keys or a query, as collection:\"old\" to rename a collection by adding the new and removing the old. add_aliases gives one paper older keys, as a manuscript cites it by, found and cited as its own; set corrects one paper's fields, as its pages.",
            "inputSchema": { "type": "object", "properties": {
                "key": { "type": "string" },
                "keys": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string" },
                "add_aliases": { "type": "array", "items": { "type": "string" } },
                "set": { "type": "object", "description": "one paper's CSL fields to set, as {\"page\": \"R89-R98\"}; null takes one away. Kept so through refresh. A title braced, {Name}, keeps its case in BibTeX." },
                "reset": { "type": "array", "items": { "type": "string" }, "description": "one paper's fields set by hand to give back to the source, for the next refresh" },
                "add_collections": { "type": "array", "items": { "type": "string" } },
                "remove_collections": { "type": "array", "items": { "type": "string" } },
                "add_labels": { "type": "array", "items": { "type": "string" } },
                "remove_labels": { "type": "array", "items": { "type": "string" } },
                "starred": { "type": "boolean" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "refresh_papers",
            "title": "Read papers again from their source",
            "description": "Read papers again from doi.org by their DOI, or arXiv, as records imported long ago may want: their pages, volume, title; their key, collections, labels, star, PDF, the fields the source lacks and those set by hand kept. Each paper's changed fields, and changes, `page: 1-11 → 1481-1491`; dry_run changes nothing.",
            "inputSchema": { "type": "object", "properties": {
                "keys": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string" },
                "dry_run": { "type": "boolean", "default": false }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        },
        {
            "name": "restore_papers",
            "title": "Bring papers back from the trash",
            "description": "Bring deleted papers back from the library's .trash/, with their PDFs; or, with pdf, a paper's PDF deleted or replaced, the PDF it has going to the trash in its place.",
            "inputSchema": { "type": "object", "required": ["keys"], "properties": {
                "keys": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                "pdf": { "type": "boolean", "default": false }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "delete_papers",
            "title": "Delete papers",
            "description": "Delete papers from the library by their keys: each record and PDF moved into the library's .trash/ folder, whence the person may move it back. Delete only papers the person asked to have deleted.",
            "inputSchema": { "type": "object", "required": ["keys"], "properties": {
                "keys": { "type": "array", "items": { "type": "string" }, "minItems": 1 }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
        },
        {
            "name": "delete_collection",
            "title": "Delete a collection or label",
            "description": "Take a collection, and the collections within it, as name/sub, off every paper in it; or a label, with kind label. Its papers stay in the library.",
            "inputSchema": { "type": "object", "required": ["name"], "properties": {
                "name": { "type": "string" },
                "kind": { "type": "string", "enum": ["collection", "label"], "default": "collection" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false }
        }
    ])
}
