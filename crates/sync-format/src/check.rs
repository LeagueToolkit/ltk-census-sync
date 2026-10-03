//! The checks a file passes before a push (`docs/APPEND.md`, "Before a push"), by what the tree's
//! own `census.yaml` names for its path: a YAML file validates against its schema, and a `.rito`
//! parses back with ltk_ritobin to a bin that prints as the same text.
//!
//! The schemas use a small part of JSON Schema: `type`, `enum`, `pattern`, `properties`,
//! `patternProperties`, `additionalProperties: false`, `required` and `items`. That part is what is
//! read here; a schema keyword outside it is refused rather than passed over.

use std::collections::HashMap;

use ltk_ritobin::{Cst, PrintCanonical as _};
use regex::Regex;
use serde_yaml_ng::{Mapping, Value};

/// The keywords the schemas use, and so the ones read.
const KEYWORDS: [&str; 13] = [
    "$schema",
    "$id",
    "title",
    "description",
    "type",
    "enum",
    "pattern",
    "properties",
    "patternProperties",
    "additionalProperties",
    "required",
    "items",
    "format",
];

/// What `census.yaml` says a tree's files are: each kind's path pattern, and how it is checked.
pub struct Census {
    kinds: Vec<(Regex, String, Rule)>,
    /// Every `pattern` and `patternProperties` key of the schemas, compiled once.
    patterns: HashMap<String, Regex>,
}

enum Rule {
    Schema(Value),
    Ritobin,
}

impl Census {
    /// The kinds a tree's `census.yaml` names, with each schema's text from `schema`, by its path
    /// in the tree.
    pub fn new(census_yaml: &str, mut schema: impl FnMut(&str) -> Result<String, String>) -> Result<Self, String> {
        let census: Value = serde_yaml_ng::from_str(census_yaml).map_err(|e| format!("census.yaml: {e}"))?;
        if census.get("format").and_then(Value::as_u64) != Some(2) {
            return Err(format!("census.yaml: format {:?}, not 2", census.get("format")));
        }
        let files = census.get("files").and_then(Value::as_mapping).ok_or("census.yaml: no files")?;
        let mut census = Census { kinds: Vec::new(), patterns: HashMap::new() };
        for (glob, rule) in files {
            let (Some(glob), Some(rule)) = (glob.as_str(), rule.as_str()) else {
                return Err(format!("census.yaml: files {glob:?}: {rule:?}"));
            };
            let rule = match rule {
                "ritobin" => Rule::Ritobin,
                path => {
                    let text = schema(path)?;
                    let value: Value = serde_yaml_ng::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
                    census.compile(&value).map_err(|e| format!("{path}: {e}"))?;
                    Rule::Schema(value)
                }
            };
            census.kinds.push((glob_regex(glob)?, glob.to_string(), rule));
        }
        Ok(census)
    }

    /// Checks one file of the tree by its kind; a path no kind names fails.
    pub fn check(&self, path: &str, text: &str) -> Result<(), String> {
        let (_, glob, rule) = self.kinds.iter().find(|(regex, ..)| regex.is_match(path)).ok_or("fits no kind of census.yaml")?;
        match rule {
            Rule::Schema(schema) => {
                let value: Value = serde_yaml_ng::from_str(text).map_err(|e| format!("does not load as YAML: {e}"))?;
                self.validate(schema, &value, "").map_err(|e| format!("{glob}: {e}"))
            }
            Rule::Ritobin => ritobin(text),
        }
    }

    /// Compiles a schema's patterns, and refuses a keyword outside the part read.
    fn compile(&mut self, schema: &Value) -> Result<(), String> {
        let Value::Mapping(map) = schema else { return Ok(()) };
        for (key, value) in map {
            let key = key.as_str().ok_or_else(|| format!("a key {key:?}"))?;
            if !KEYWORDS.contains(&key) {
                return Err(format!("the keyword {key:?} is not one this check reads"));
            }
            match (key, value) {
                ("pattern", Value::String(pattern)) => self.pattern(pattern)?,
                ("properties", Value::Mapping(properties)) => {
                    for schema in properties.values() {
                        self.compile(schema)?;
                    }
                }
                ("patternProperties", Value::Mapping(properties)) => {
                    for (pattern, schema) in properties {
                        self.pattern(pattern.as_str().ok_or_else(|| format!("a pattern {pattern:?}"))?)?;
                        self.compile(schema)?;
                    }
                }
                ("items", schema) => self.compile(schema)?,
                _ => {}
            }
        }
        Ok(())
    }

    fn pattern(&mut self, pattern: &str) -> Result<(), String> {
        let regex = Regex::new(pattern).map_err(|e| format!("pattern {pattern:?}: {e}"))?;
        self.patterns.insert(pattern.to_string(), regex);
        Ok(())
    }

    /// The first way `value` breaks `schema`, at `at` (a path of keys and indices).
    fn validate(&self, schema: &Value, value: &Value, at: &str) -> Result<(), String> {
        let place = || if at.is_empty() { "the file".to_string() } else { at.to_string() };
        if let Some(types) = schema.get("type") {
            let fits = match types {
                Value::String(name) => is_type(value, name),
                Value::Sequence(names) => names.iter().any(|n| n.as_str().is_some_and(|n| is_type(value, n))),
                _ => false,
            };
            if !fits {
                return Err(format!("{} is not {}", place(), serde_yaml_ng::to_string(types).unwrap_or_default().trim()));
            }
        }
        if let Some(Value::Sequence(options)) = schema.get("enum")
            && !options.contains(value)
        {
            return Err(format!("{} is none of {}", place(), serde_yaml_ng::to_string(options).unwrap_or_default().trim().replace('\n', " ")));
        }
        if let (Some(Value::String(pattern)), Value::String(text)) = (schema.get("pattern"), value)
            && !self.patterns[pattern].is_match(text)
        {
            return Err(format!("{} {text:?} does not match {pattern}", place()));
        }
        if let Value::Mapping(map) = value {
            self.validate_mapping(schema, map, at)?;
        }
        if let (Some(items), Value::Sequence(values)) = (schema.get("items"), value) {
            for (i, item) in values.iter().enumerate() {
                self.validate(items, item, &format!("{at}/{i}"))?;
            }
        }
        Ok(())
    }

    fn validate_mapping(&self, schema: &Value, map: &Mapping, at: &str) -> Result<(), String> {
        for required in schema.get("required").and_then(Value::as_sequence).into_iter().flatten() {
            if !map.contains_key(required) {
                return Err(format!("{at}/{} is missing", required.as_str().unwrap_or_default()));
            }
        }
        let properties = schema.get("properties").and_then(Value::as_mapping);
        let pattern_properties = schema.get("patternProperties").and_then(Value::as_mapping);
        let closed = schema.get("additionalProperties") == Some(&Value::Bool(false));
        for (key, value) in map {
            let key = key.as_str().ok_or_else(|| format!("{at}: the key {key:?} is not a string"))?;
            let here = format!("{at}/{key}");
            let mut named = false;
            if let Some(schema) = properties.and_then(|p| p.get(key)) {
                named = true;
                self.validate(schema, value, &here)?;
            }
            for (pattern, schema) in pattern_properties.into_iter().flatten() {
                if pattern.as_str().is_some_and(|p| self.patterns[p].is_match(key)) {
                    named = true;
                    self.validate(schema, value, &here)?;
                }
            }
            if closed && !named {
                return Err(format!("{here} is not a field the schema has"));
            }
        }
        Ok(())
    }
}

fn is_type(value: &Value, name: &str) -> bool {
    match name {
        "object" => value.is_mapping(),
        "array" => value.is_sequence(),
        "string" => value.is_string(),
        "integer" => value.as_u64().is_some() || value.as_i64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_bool(),
        "null" => value.is_null(),
        _ => false,
    }
}

/// A `census.yaml` path pattern as a regex: `**/` any directories, `?` and `*` within one name.
fn glob_regex(glob: &str) -> Result<Regex, String> {
    let mut regex = String::from("^");
    let mut rest = glob;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("**/") {
            regex.push_str("(?:[^/]+/)*");
            rest = after;
            continue;
        }
        match c {
            '?' => regex.push_str("[^/]"),
            '*' => regex.push_str("[^/]*"),
            c => regex.push_str(&regex::escape(&c.to_string())),
        }
        rest = &rest[c.len_utf8()..];
    }
    regex.push('$');
    Regex::new(&regex).map_err(|e| format!("census.yaml: the pattern {glob:?}: {e}"))
}

/// Whether ritobin text parses with no error, builds a bin with no diagnostic, and that bin prints
/// as the same text.
fn ritobin(text: &str) -> Result<(), String> {
    let cst = Cst::parse(text);
    if let Some(error) = cst.errors.first() {
        return Err(format!("does not parse as ritobin ({} errors): {error:?}", cst.errors.len()));
    }
    let partial = cst.build_bin(text);
    if let Some(diagnostic) = partial.diagnostics.first() {
        return Err(format!("does not build a bin ({} diagnostics): {diagnostic:?}", partial.diagnostics.len()));
    }
    let printed = partial.bin.print_canonical().map_err(|e| format!("does not print back: {e}"))?;
    if printed != text {
        let line = printed.lines().zip(text.lines()).position(|(a, b)| a != b).unwrap_or(printed.lines().count().min(text.lines().count()));
        return Err(format!("prints back as other text, from line {}", line + 1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glob_matches_by_names() {
        let entry = glob_regex("files/**/??/????????????????.yaml").unwrap();
        assert!(entry.is_match("files/data/final/a.wad.client/01/014ca4df836b3e73.yaml"));
        assert!(!entry.is_match("files/data/final/a.wad.client/_wad.yaml"));
        assert!(!entry.is_match("files/data/final/a.wad.client/01/014ca4df836b3e73.bin/35/3537586f.yaml"));
        let rito = glob_regex("files/**/??/????????????????.bin/??/????????.rito").unwrap();
        assert!(rito.is_match("files/x.wad.client/01/014ca4df836b3e73.bin/35/3537586f.rito"));
        assert!(glob_regex("build.yaml").unwrap().is_match("build.yaml"));
        assert!(!glob_regex("build.yaml").unwrap().is_match("buildxyaml"));
    }
}
