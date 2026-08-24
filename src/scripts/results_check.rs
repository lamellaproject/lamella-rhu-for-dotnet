//! Validator for gate results files (JSON-lines, schema 1).

use std::env;
use std::fs;
use std::process::ExitCode;

const SCHEMA: i64 = 1;
const VERDICTS: [&str; 3] = ["pass", "fail", "skip"];
const CONTROLS: [&str; 2] = ["positive", "negative"];
const COMPARES: [&str; 2] = ["bytes", "pins"];

const RUN_REQUIRED: [&str; 7] = ["schema", "type", "gate", "run", "host", "runner", "tools"];
const CASE_REQUIRED: [&str; 6] = ["schema", "type", "gate", "case", "verdict", "control"];
const SUMMARY_REQUIRED: [&str; 10] = [
    "schema", "type", "gate", "verdict", "cases", "pass", "fail", "skip", "neg_controls", "exit",
];
const RUN_KNOWN: [&str; 9] = [
    "schema", "type", "gate", "run", "host", "runner", "tools", "chain", "notes",
];
const CASE_KNOWN: [&str; 14] = [
    "schema", "type", "gate", "case", "verdict", "control", "timing", "exit", "exit_want",
    "image", "golden", "broke", "skip_reason", "stats",
];
const TIMING_KNOWN: [&str; 1] = ["icount-shift0"];
const SUSPECT_STAT_KEYS: [&str; 6] = ["verdict", "exit", "exit_want", "pass", "fail", "want"];


#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get<'a>(&'a self, key: &str) -> Option<&'a Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    fn as_int(&self) -> Option<i64> {
        match self {
            Json::Int(n) => Some(*n),
            _ => None,
        }
    }
    fn keys(&self) -> Vec<&str> {
        match self {
            Json::Obj(pairs) => pairs.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Parser { bytes: text.as_bytes(), pos: 0 }
    }

    fn parse(text: &str) -> Result<Json, String> {
        let mut p = Parser::new(text);
        p.skip_ws();
        let v = p.value()?;
        p.skip_ws();
        if p.pos != p.bytes.len() {
            return Err(format!("trailing data at offset {}", p.pos));
        }
        Ok(v)
    }

    fn skip_ws(&mut self) {
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.bytes.get(self.pos) == Some(&b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at offset {}", b as char, self.pos))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(format!("bad literal at offset {}", self.pos))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.bytes.get(self.pos) {
            None => Err("unexpected end of input".to_string()),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(_) => self.number(),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut pairs: Vec<(String, Json)> = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Json::Obj(pairs));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let val = self.value()?;
            match pairs.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = val,
                None => pairs.push((key, val)),
            }
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(pairs));
                }
                _ => return Err(format!("expected ',' or '}}' at offset {}", self.pos)),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(format!("expected ',' or ']' at offset {}", self.pos)),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.bytes.get(self.pos) {
                None => return Err("unterminated string".to_string()),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let esc = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| "unterminated escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.pos..self.pos + 4)
                                .ok_or_else(|| "truncated \\u escape".to_string())?;
                            let hex = std::str::from_utf8(hex)
                                .map_err(|_| "bad \\u escape".to_string())?;
                            let code = u32::from_str_radix(hex, 16)
                                .map_err(|_| "bad \\u escape".to_string())?;
                            self.pos += 4;
                            let ch = char::from_u32(code)
                                .ok_or_else(|| "unsupported \\u escape".to_string())?;
                            out.push(ch);
                        }
                        _ => return Err(format!("bad escape '\\{}'", esc as char)),
                    }
                }
                Some(&b) if b < 0x20 => {
                    return Err("control byte inside string".to_string());
                }
                Some(&b) => {
                    out.push(b as char);
                    self.pos += 1;
                }
            }
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        let mut is_float = false;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'0'..=b'9' => self.pos += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    is_float = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| "bad number".to_string())?;
        if text.is_empty() || text == "-" {
            return Err(format!("bad number at offset {}", start));
        }
        if is_float {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|_| format!("bad number '{}'", text))
        } else {
            match text.parse::<i64>() {
                Ok(n) => Ok(Json::Int(n)),
                Err(_) => text
                    .parse::<f64>()
                    .map(Json::Float)
                    .map_err(|_| format!("bad number '{}'", text)),
            }
        }
    }
}


fn is_nonempty_str(v: Option<&Json>) -> bool {
    matches!(v, Some(Json::Str(s)) if !s.is_empty())
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_gate_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn check_hash_obj(obj: &Json, name: &str, where_: &str, errors: &mut Vec<String>) {
    if !matches!(obj, Json::Obj(_)) {
        errors.push(format!("{}: {} must be an object or null", where_, name));
        return;
    }
    if !is_nonempty_str(obj.get("path")) {
        errors.push(format!("{}: {}.path missing or empty", where_, name));
    }
    match obj.get("sha256").and_then(|v| v.as_str()) {
        Some(s) if is_sha256(s) => {}
        _ => errors.push(format!(
            "{}: {}.sha256 is not 64 lowercase hex chars",
            where_, name
        )),
    }
}

fn validate_lines(lines: &[String], path: &str) -> (Vec<String>, Vec<String>) {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut records: Vec<(usize, Json)> = Vec::new();

    for (i, raw) in lines.iter().enumerate() {
        let idx = i + 1;
        let where_ = format!("{}:{}", path, idx);
        if raw.trim().is_empty() {
            errors.push(format!("{}: blank line (one JSON object per line)", where_));
            continue;
        }
        match Parser::parse(raw) {
            Err(e) => errors.push(format!("{}: not valid JSON ({})", where_, e)),
            Ok(obj) => {
                if !matches!(obj, Json::Obj(_)) {
                    errors.push(format!("{}: line is not a JSON object", where_));
                    continue;
                }
                records.push((idx, obj));
            }
        }
    }

    if records.is_empty() {
        errors.push(format!("{}: no records", path));
        return (errors, warnings);
    }

    let (first_idx, first) = &records[0];
    if first.get("type").and_then(|v| v.as_str()) != Some("run") {
        errors.push(format!(
            "{}:{}: first record must be type \"run\"",
            path, first_idx
        ));
    }
    let (last_idx, last) = &records[records.len() - 1];
    if last.get("type").and_then(|v| v.as_str()) != Some("summary") {
        errors.push(format!(
            "{}:{}: last record must be type \"summary\"",
            path, last_idx
        ));
    }

    let gate: Option<String> = first
        .get("gate")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut n_run = 0usize;
    let mut n_summary = 0usize;
    let mut tally_pass = 0i64;
    let mut tally_fail = 0i64;
    let mut tally_skip = 0i64;
    let mut n_negative = 0i64;
    let mut case_ids: Vec<String> = Vec::new();
    let mut summary: Option<Json> = None;
    let mut summary_where = String::new();

    for (idx, obj) in &records {
        let where_ = format!("{}:{}", path, idx);
        let rtype = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let known: &[&str] = match rtype {
            "run" => &RUN_KNOWN,
            "case" => &CASE_KNOWN,
            "summary" => &SUMMARY_REQUIRED,
            _ => {
                let shown = match obj.get("type") {
                    Some(Json::Str(s)) => format!("'{}'", s),
                    Some(other) => format!("{:?}", other),
                    None => "None".to_string(),
                };
                errors.push(format!("{}: unknown type {}", where_, shown));
                continue;
            }
        };

        if obj.get("schema").and_then(|v| v.as_int()) != Some(SCHEMA) {
            errors.push(format!("{}: schema must be {}", where_, SCHEMA));
        }
        match obj.get("gate").and_then(|v| v.as_str()) {
            Some(g) if is_gate_name(g) => {
                if let Some(run_gate) = &gate {
                    if g != run_gate {
                        errors.push(format!(
                            "{}: gate '{}' differs from run line gate '{}'",
                            where_, g, run_gate
                        ));
                    }
                }
            }
            _ => errors.push(format!("{}: gate missing or not [a-z0-9-]+", where_)),
        }

        let unknown: Vec<&str> = {
            let mut u: Vec<&str> = obj
                .keys()
                .into_iter()
                .filter(|k| !known.contains(k))
                .collect();
            u.sort_unstable();
            u
        };
        if !unknown.is_empty() {
            warnings.push(format!(
                "{}: unknown field(s) {} (tolerated)",
                where_,
                unknown.join(", ")
            ));
        }

        match rtype {
            "run" => {
                n_run += 1;
                for field in RUN_REQUIRED {
                    if obj.get(field).is_none() {
                        errors.push(format!("{}: run line missing '{}'", where_, field));
                    }
                }
                match obj.get("tools") {
                    Some(Json::Obj(pairs)) if !pairs.is_empty() => {
                        let all_ok = pairs
                            .iter()
                            .all(|(_, v)| matches!(v, Json::Str(s) if !s.is_empty()));
                        if !all_ok {
                            errors.push(format!(
                                "{}: tools values must be non-empty strings",
                                where_
                            ));
                        }
                    }
                    Some(_) | None => {
                        if obj.get("tools").is_some() {
                            errors.push(format!("{}: tools must be a non-empty object", where_));
                        }
                    }
                }
                for field in ["run", "host", "runner"] {
                    if let Some(v) = obj.get(field) {
                        if !matches!(v, Json::Str(s) if !s.is_empty()) {
                            errors.push(format!("{}: {} must be a non-empty string", where_, field));
                        }
                    }
                }
            }
            "case" => {
                for field in CASE_REQUIRED {
                    if obj.get(field).is_none() {
                        errors.push(format!("{}: case line missing '{}'", where_, field));
                    }
                }
                match obj.get("case").and_then(|v| v.as_str()) {
                    Some(cid) if !cid.is_empty() => {
                        if case_ids.iter().any(|c| c == cid) {
                            errors.push(format!("{}: duplicate case id '{}'", where_, cid));
                        } else {
                            case_ids.push(cid.to_string());
                        }
                    }
                    _ => errors.push(format!("{}: case id missing or empty", where_)),
                }

                let verdict = obj.get("verdict").and_then(|v| v.as_str());
                match verdict {
                    Some("pass") => tally_pass += 1,
                    Some("fail") => tally_fail += 1,
                    Some("skip") => tally_skip += 1,
                    _ => errors.push(format!(
                        "{}: verdict {} not in {:?}",
                        where_,
                        json_repr(obj.get("verdict")),
                        VERDICTS
                    )),
                }

                let control = obj.get("control").and_then(|v| v.as_str());
                match control {
                    Some("positive") => {}
                    Some("negative") => {
                        n_negative += 1;
                        if !is_nonempty_str(obj.get("broke")) {
                            errors.push(format!(
                                "{}: negative control requires non-empty \"broke\"",
                                where_
                            ));
                        }
                    }
                    _ => errors.push(format!(
                        "{}: control {} not in {:?}",
                        where_,
                        json_repr(obj.get("control")),
                        CONTROLS
                    )),
                }

                for field in ["exit", "exit_want"] {
                    if let Some(v) = obj.get(field) {
                        if !matches!(v, Json::Null | Json::Int(_)) {
                            errors.push(format!(
                                "{}: {} must be an integer or null",
                                where_, field
                            ));
                        }
                    }
                }

                if let Some(t) = obj.get("timing") {
                    match t.as_str() {
                        Some(name) if TIMING_KNOWN.contains(&name) => {}
                        _ => errors.push(format!(
                            "{}: timing {} not a known execution profile {:?}",
                            where_,
                            json_repr(Some(t)),
                            TIMING_KNOWN
                        )),
                    }
                }

                if verdict == Some("skip") {
                    if !is_nonempty_str(obj.get("skip_reason")) {
                        errors.push(format!(
                            "{}: skip requires non-empty \"skip_reason\"",
                            where_
                        ));
                    }
                } else if !matches!(obj.get("skip_reason"), None | Some(Json::Null)) {
                    errors.push(format!("{}: skip_reason present on non-skip case", where_));
                }

                if let Some(image) = obj.get("image") {
                    if !matches!(image, Json::Null) {
                        check_hash_obj(image, "image", &where_, &mut errors);
                    }
                }
                if let Some(golden) = obj.get("golden") {
                    if !matches!(golden, Json::Null) {
                        check_hash_obj(golden, "golden", &where_, &mut errors);
                        if matches!(golden, Json::Obj(_)) {
                            match golden.get("compare").and_then(|v| v.as_str()) {
                                Some(c) if COMPARES.contains(&c) => {}
                                _ => errors.push(format!(
                                    "{}: golden.compare must be one of {:?}",
                                    where_, COMPARES
                                )),
                            }
                        }
                    }
                }

                if let Some(stats) = obj.get("stats") {
                    match stats {
                        Json::Null => {}
                        Json::Obj(pairs) => {
                            let mut suspect: Vec<&str> = pairs
                                .iter()
                                .map(|(k, _)| k.as_str())
                                .filter(|k| SUSPECT_STAT_KEYS.contains(&k.to_lowercase().as_str()))
                                .collect();
                            suspect.sort_unstable();
                            if !suspect.is_empty() {
                                warnings.push(format!(
                                    "{}: stats key(s) {} look load-bearing; stats are report-only",
                                    where_,
                                    suspect.join(", ")
                                ));
                            }
                        }
                        _ => errors.push(format!("{}: stats must be an object or null", where_)),
                    }
                }
            }
            "summary" => {
                n_summary += 1;
                summary = Some(obj.clone());
                summary_where = where_.clone();
                for field in SUMMARY_REQUIRED {
                    if obj.get(field).is_none() {
                        errors.push(format!("{}: summary missing '{}'", where_, field));
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    if n_run != 1 {
        errors.push(format!(
            "{}: expected exactly 1 run line, found {}",
            path, n_run
        ));
    }
    if n_summary != 1 {
        errors.push(format!(
            "{}: expected exactly 1 summary line, found {}",
            path, n_summary
        ));
    }
    if case_ids.is_empty() {
        errors.push(format!("{}: no case lines", path));
    }

    if let (Some(s), 1) = (&summary, n_summary) {
        let cases_want = if case_ids.is_empty() {
            0
        } else {
            tally_pass + tally_fail + tally_skip
        };
        let expects = [
            ("cases", cases_want),
            ("pass", tally_pass),
            ("fail", tally_fail),
            ("skip", tally_skip),
            ("neg_controls", n_negative),
        ];
        for (field, want) in expects {
            match s.get(field).and_then(|v| v.as_int()) {
                Some(got) if got != want => errors.push(format!(
                    "{}: summary {}={} but case lines tally {}",
                    summary_where, field, got, want
                )),
                Some(_) => {}
                None => errors.push(format!(
                    "{}: summary {} must be an integer",
                    summary_where, field
                )),
            }
        }
        let verdict = s.get("verdict").and_then(|v| v.as_str());
        let want_verdict = if tally_fail > 0 { "fail" } else { "pass" };
        match verdict {
            Some(v @ ("pass" | "fail")) => {
                if v != want_verdict {
                    errors.push(format!(
                        "{}: summary verdict '{}' contradicts case tallies (want '{}')",
                        summary_where, v, want_verdict
                    ));
                }
            }
            _ => errors.push(format!(
                "{}: summary verdict must be \"pass\" or \"fail\"",
                summary_where
            )),
        }
        if let Some(gexit) = s.get("exit").and_then(|v| v.as_int()) {
            match verdict {
                Some("pass") if gexit != 0 => errors.push(format!(
                    "{}: summary verdict pass but exit {}",
                    summary_where, gexit
                )),
                Some("fail") if gexit == 0 => errors.push(format!(
                    "{}: summary verdict fail but exit 0",
                    summary_where
                )),
                _ => {}
            }
        }
    }

    (errors, warnings)
}

fn json_repr(v: Option<&Json>) -> String {
    match v {
        None => "None".to_string(),
        Some(Json::Str(s)) => format!("'{}'", s),
        Some(Json::Null) => "None".to_string(),
        Some(Json::Bool(b)) => format!("{}", b),
        Some(Json::Int(n)) => format!("{}", n),
        Some(Json::Float(f)) => format!("{}", f),
        Some(other) => format!("{:?}", other),
    }
}

fn validate_file(path: &str) -> (Vec<String>, Vec<String>) {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return (vec![format!("{}: unreadable ({})", path, e)], Vec::new()),
    };
    if let Some(pos) = bytes.iter().position(|&b| b > 0x7F) {
        return (
            vec![format!(
                "{}: unreadable (non-ASCII byte 0x{:02x} at offset {})",
                path, bytes[pos], pos
            )],
            Vec::new(),
        );
    }
    let text = String::from_utf8(bytes).expect("ASCII verified above");
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();
    if lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    validate_lines(&lines, path)
}


fn good_fixture() -> Vec<String> {
    let sha_a = "a".repeat(64);
    let sha_b = "b".repeat(64);
    vec![
        concat!(
            "{\"schema\":1,\"type\":\"run\",\"gate\":\"base\",\"run\":\"base-1\",",
            "\"host\":\"qemu-mps2-an500\",\"runner\":\"src/scripts/base-gate.sh\",",
            "\"tools\":{\"qemu\":\"QEMU emulator version 8.2.2\"}}"
        )
        .to_string(),
        format!(
            concat!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"base\",\"case\":\"boot\",",
                "\"verdict\":\"pass\",\"exit\":0,\"exit_want\":0,\"control\":\"positive\",",
                "\"image\":{{\"path\":\"target/gates/base-boot.elf\",\"sha256\":\"{}\"}}}}"
            ),
            sha_a
        ),
        format!(
            concat!(
                "{{\"schema\":1,\"type\":\"case\",\"gate\":\"base\",\"case\":\"fault\",",
                "\"verdict\":\"pass\",\"exit\":2,\"exit_want\":2,\"control\":\"negative\",",
                "\"broke\":\"deliberate udf\",",
                "\"image\":{{\"path\":\"target/gates/base-fault.elf\",\"sha256\":\"{}\"}}}}"
            ),
            sha_b
        ),
        concat!(
            "{\"schema\":1,\"type\":\"case\",\"gate\":\"base\",\"case\":\"soak\",",
            "\"verdict\":\"skip\",\"exit\":null,\"exit_want\":null,\"control\":\"positive\",",
            "\"image\":null,\"skip_reason\":\"soak runs on silicon legs only\"}"
        )
        .to_string(),
        concat!(
            "{\"schema\":1,\"type\":\"summary\",\"gate\":\"base\",\"verdict\":\"pass\",",
            "\"cases\":3,\"pass\":2,\"fail\":0,\"skip\":1,\"neg_controls\":1,\"exit\":0}"
        )
        .to_string(),
    ]
}

fn mutate(good: &[String], index: usize, replacement: Option<String>) -> Vec<String> {
    let mut lines: Vec<String> = good.to_vec();
    match replacement {
        None => {
            lines.remove(index);
        }
        Some(r) => lines[index] = r,
    }
    lines
}

fn self_test() -> u8 {
    let good = good_fixture();
    let mut failed = false;

    let (errors, _) = validate_lines(&good, "good");
    if !errors.is_empty() {
        failed = true;
        println!("SELF-TEST FAIL: good fixture rejected:");
        for e in &errors {
            println!("  {}", e);
        }
    } else {
        println!("self-test: good fixture accepted");
    }

    let stats_line = format!("{},\"stats\":{{\"exit\":1}}}}", &good[1][..good[1].len() - 1]);
    let (_, warnings) = validate_lines(&mutate(&good, 1, Some(stats_line)), "statwarn");
    if !warnings.iter().any(|w| w.contains("load-bearing")) {
        failed = true;
        println!("SELF-TEST FAIL: load-bearing stats key drew no warning");
    } else {
        println!("self-test: load-bearing stats key warned");
    }

    let bad: Vec<(&str, Vec<String>)> = vec![
        ("missing summary", good[..good.len() - 1].to_vec()),
        ("run line not first", {
            let mut v = vec![good[1].clone(), good[0].clone()];
            v.extend_from_slice(&good[2..]);
            v
        }),
        (
            "tally mismatch",
            mutate(
                &good,
                4,
                Some(
                    concat!(
                        "{\"schema\":1,\"type\":\"summary\",\"gate\":\"base\",",
                        "\"verdict\":\"pass\",\"cases\":3,\"pass\":3,\"fail\":0,",
                        "\"skip\":0,\"neg_controls\":1,\"exit\":0}"
                    )
                    .to_string(),
                ),
            ),
        ),
        (
            "lying summary verdict",
            mutate(
                &good,
                2,
                Some(good[2].replace("\"verdict\":\"pass\"", "\"verdict\":\"fail\"")),
            ),
        ),
        (
            "negative control without broke",
            mutate(
                &good,
                2,
                Some(good[2].replace(",\"broke\":\"deliberate udf\"", "")),
            ),
        ),
        (
            "skip without reason",
            mutate(
                &good,
                3,
                Some(good[3].replace(",\"skip_reason\":\"soak runs on silicon legs only\"", "")),
            ),
        ),
        (
            "duplicate case id",
            mutate(
                &good,
                3,
                Some(good[3].replace("\"case\":\"soak\"", "\"case\":\"boot\"")),
            ),
        ),
        (
            "bad verdict value",
            mutate(
                &good,
                1,
                Some(good[1].replace("\"verdict\":\"pass\"", "\"verdict\":\"green\"")),
            ),
        ),
        (
            "bad image hash",
            mutate(
                &good,
                1,
                Some(good[1].replace(&"a".repeat(64), &"a".repeat(10))),
            ),
        ),
        ("not json", mutate(&good, 1, Some("PASS boot".to_string()))),
        (
            "wrong schema",
            mutate(
                &good,
                1,
                Some(good[1].replace("\"schema\":1", "\"schema\":2")),
            ),
        ),
        (
            "cross-gate case line",
            mutate(
                &good,
                1,
                Some(good[1].replace("\"gate\":\"base\"", "\"gate\":\"sched\"")),
            ),
        ),
    ];

    for (name, lines) in &bad {
        let (errors, _) = validate_lines(lines, name);
        if !errors.is_empty() {
            println!(
                "self-test: rejected \"{}\" ({} error{})",
                name,
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            );
        } else {
            failed = true;
            println!("SELF-TEST FAIL: \"{}\" was accepted", name);
        }
    }

    println!("SELF-TEST {}", if failed { "FAIL" } else { "PASS" });
    if failed {
        1
    } else {
        0
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("usage: results-check FILE...     validate results files");
        println!("       results-check --self-test run the embedded fixture set");
        println!("exit codes: 0 = all files valid, 1 = violations, 2 = usage error");
        return ExitCode::from(2);
    }
    if args[1] == "--self-test" {
        return ExitCode::from(self_test());
    }

    let mut any_invalid = false;
    for path in &args[1..] {
        let (errors, warnings) = validate_file(path);
        for w in &warnings {
            println!("WARN: {}", w);
        }
        for e in &errors {
            println!("ERROR: {}", e);
        }
        if !errors.is_empty() {
            any_invalid = true;
            println!(
                "{}: INVALID ({} error{})",
                path,
                errors.len(),
                if errors.len() == 1 { "" } else { "s" }
            );
        } else {
            println!("{}: OK", path);
        }
    }
    ExitCode::from(if any_invalid { 1 } else { 0 })
}
