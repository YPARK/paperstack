//! paperstack as an MCP server, for an AI agent to search the library,
//! read its papers, cite them, add to it and delete from it: `paperstack mcp`, run by the
//! agent's program, as Cursor or Claude Code, which speaks to it in
//! JSON-RPC, a message a line, on its stdin and stdout. Nothing else is
//! written to stdout; what goes wrong is said on stderr.
//!
//! The index is brought up to the library at each call, so what the TUI,
//! `paperstack serve` or another machine added meanwhile is found.

use crate::add::{Adder, Options, attach, one_line};
use crate::bibtex;
use crate::index::{Hit, Index, Sort};
use crate::library::Library;
use crate::net::Net;
use crate::query::Query;
use anyhow::{Context, bail};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// The protocol's version answered when the client's is not one known.
const PROTOCOL: &str = "2025-06-18";
const KNOWN: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];
/// The most pages `read_paper` gives at once, and characters of them.
const PAGES_AT_ONCE: u32 = 20;
const TEXT_LIMIT: usize = 200_000;

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
            Ok(data) => {
                let text = match &data {
                    Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                let mut result =
                    json!({ "content": [{ "type": "text", "text": text }], "isError": false });
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
                let mut hits: Vec<Hit> = Vec::new();
                for key in list("keys") {
                    hits.push(self.hit(&key)?);
                }
                if let Some(q) = text("query") {
                    hits.extend(self.index.search(
                        &Query::parse(&q),
                        Sort::Relevance,
                        false,
                        None,
                    )?);
                }
                if hits.is_empty() {
                    bail!("give keys, or a query, of the papers to cite");
                }
                let records: Vec<_> = hits.iter().map(|h| h.record.as_ref()).collect();
                Ok(Value::String(match text("format").as_deref() {
                    Some("csl") => serde_json::to_string_pretty(
                        &records.iter().map(|r| r.item()).collect::<Vec<_>>(),
                    )?,
                    Some("keys") => format!(
                        "[{}]",
                        records
                            .iter()
                            .map(|r| format!("@{}", r.key))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ),
                    _ => bibtex::entries(&records),
                }))
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
                let key = text("key").context("give the paper's key")?;
                let path = crate::browser::dropped(&text("path").context("give the PDF's path")?);
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
                Ok(json!({ "outcome": added.summary(), "key": added.key }))
            }
            "update_paper" => {
                let hit = self.hit(&text("key").context("give the paper's key")?)?;
                let mut record = crate::library::read(&self.lib.root.join(&hit.path))?;
                let m = &mut record.paperstack;
                for (have, add, remove) in [
                    (
                        &mut m.folders,
                        list("add_collections"),
                        list("remove_collections"),
                    ),
                    (&mut m.labels, list("add_labels"), list("remove_labels")),
                ] {
                    have.retain(|h| !remove.iter().any(|r| r.eq_ignore_ascii_case(h)));
                    for a in add {
                        if !have.iter().any(|h| h.eq_ignore_ascii_case(&a)) {
                            have.push(a);
                        }
                    }
                }
                if let Some(star) = args.get("starred").and_then(Value::as_bool) {
                    m.starred = star;
                }
                let path = self.lib.save(&record, Some(&hit.path))?;
                self.index
                    .upsert(&self.lib, &path, record, Some(&hit.path))?;
                let hit = self.hit(&hit.record.key)?;
                Ok(self.paper(&hit, false))
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

    /// A paper as the agent is given it; with its abstract if `whole`.
    fn paper(&self, h: &Hit, whole: bool) -> Value {
        let mut v = crate::cli::json(&self.lib, h);
        if let Value::Object(map) = &mut v {
            if let Some(f) = map.remove("folders") {
                map.insert("collections".to_string(), f);
            }
            if whole {
                map.insert("abstract".to_string(), json!(h.record.abstract_text()));
                map.insert("bibtex".to_string(), json!(bibtex::entry(&h.record)));
            }
        }
        v
    }
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
            "description": "Find papers in the library. Words and \"phrases\" are looked for in titles, abstracts, authors, venues and keys, a word matching the words it starts. Fields: title: abstract: author: venue: year:2020..2026 collection:\"name\" label: key: doi: has:pdf has:abstract is:starred; - before any leaves out what it matches. Empty, it lists every paper.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "e.g. perturbation \"linear baseline\" author:Huber year:2020.." },
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
            "description": "BibTeX, CSL JSON, or [@key; @key] citations, of the papers with the keys given, or those a query finds, or both.",
            "inputSchema": { "type": "object", "properties": {
                "keys": { "type": "array", "items": { "type": "string" } },
                "query": { "type": "string" },
                "format": { "type": "string", "enum": ["bibtex", "csl", "keys"], "default": "bibtex" }
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
            "description": "Add a paper by its DOI, arXiv id, PMID, a publisher's address, a PDF's path, or its exact title, its record read from Crossref, arXiv or PubMed and an open copy of its PDF downloaded. A paper already here is not added again. A title several papers have adds none, and lists them, to add by id.",
            "inputSchema": { "type": "object", "required": ["what"], "properties": {
                "what": { "type": "string", "description": "e.g. 10.1038/s41586-020-2649-2, arXiv:2412.08821, PMID: 28693043" },
                "collections": { "type": "array", "items": { "type": "string" } },
                "labels": { "type": "array", "items": { "type": "string" } },
                "pdf": { "type": "boolean", "default": true, "description": "download an open copy of the PDF" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true }
        },
        {
            "name": "attach_pdf",
            "title": "Give a paper its PDF",
            "description": "Give a paper without one the PDF at a path on this machine, as one downloaded in a browser: only if its first page has the paper's title, or, for a scan with too little text, the paper's DOI. A scan without it is refused: the person may attach it themselves, by `paperstack attach KEY FILE`.",
            "inputSchema": { "type": "object", "required": ["key", "path"], "properties": {
                "key": { "type": "string" },
                "path": { "type": "string" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
        },
        {
            "name": "update_paper",
            "title": "Change a paper's collections, labels or star",
            "description": "Put a paper in collections or take it out, add or take away labels, star it or not.",
            "inputSchema": { "type": "object", "required": ["key"], "properties": {
                "key": { "type": "string" },
                "add_collections": { "type": "array", "items": { "type": "string" } },
                "remove_collections": { "type": "array", "items": { "type": "string" } },
                "add_labels": { "type": "array", "items": { "type": "string" } },
                "remove_labels": { "type": "array", "items": { "type": "string" } },
                "starred": { "type": "boolean" }
            } },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false }
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
