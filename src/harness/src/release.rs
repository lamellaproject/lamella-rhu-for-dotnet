//! The repository's record of its frozen tiers and the release's
//! archives: `goldens/MANIFEST.md` names the five tiers' own manifests with
//! their sha256s and where each tier ships, and the release's archives (the
//! two hosted-runtime tiers and the two runtime archive sets, in
//! deterministic tar archives) with their sha256s and member lists. Each
//! archive carries the same list as `MANIFEST.txt` at its root, generated
//! from the same members, so the record and the archive cannot drift. This
//! module builds the record from a tree, checks a committed record against
//! a tree, and packs the archives; the runner's selfcheck runs the checker
//! on the checkout and on fixtures in both directions.
//!
//! An archive is POSIX ustar: 512-byte blocks, the members sorted bytewise
//! by path, mode 0644, owner and group 0, modification time 0, no owner
//! names, no directory entries, two zero blocks at the end. The same tree
//! writes the same bytes on every host, so the tree can pin each archive's
//! sha256 and the selfcheck can rebuild an archive and refuse a record that
//! does not tie the tree.

use crate::gates::rt::{RtWorld, WORLD_M33, WORLD_M7};
use crate::sha256::sha256_hex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::exit;

/// The record's file in the tree.
pub const RECORD: &str = "goldens/MANIFEST.md";

/// The list each archive carries at its root.
pub const ARCHIVE_MANIFEST: &str = "MANIFEST.txt";

/// A tree the record is built from or checked against: a checkout on
/// disk, or a fixture in memory.
pub trait Tree {
    /// The names of the files directly under `dir` (an empty string is the
    /// root), sorted bytewise; `None` when the directory is absent.
    fn files(&self, dir: &str) -> Option<Vec<String>>;
    /// A file's bytes; `None` when it is absent.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// A directory on disk as a tree.
pub struct FsTree {
    pub root: PathBuf,
}

impl Tree for FsTree {
    fn files(&self, dir: &str) -> Option<Vec<String>> {
        let at = if dir.is_empty() { self.root.clone() } else { self.root.join(dir.replace('/', std::path::MAIN_SEPARATOR_STR)) };
        let entries = std::fs::read_dir(&at).ok()?;
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        Some(names)
    }
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path.replace('/', std::path::MAIN_SEPARATOR_STR))).ok()
    }
}

/// A fixture tree: every file by its path.
pub struct MapTree(pub BTreeMap<String, Vec<u8>>);

impl Tree for MapTree {
    fn files(&self, dir: &str) -> Option<Vec<String>> {
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        let names: Vec<String> = self
            .0
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .map(|k| &k[prefix.len()..])
            .filter(|rest| !rest.is_empty() && !rest.contains('/'))
            .map(str::to_string)
            .collect();
        if names.is_empty() { None } else { Some(names) }
    }
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.get(path).cloned()
    }
}

/// The five tiers in the chain's order.
const TIERS: [(&str, &str); 5] = [
    ("gc", "the garbage-collector tier"),
    ("shim", "the shim tier"),
    ("payload", "the payload tier"),
    ("rt-m7", "the Cortex-M7 hosted-runtime tier"),
    ("rt-m33", "the Cortex-M33 hosted-runtime tier"),
];

/// The release's archives, from this table alone. Each archive holds some
/// tiers (a directory per tier inside, `goldens/<tier>/...`, unpacked at
/// the repository root) and some runtime archive sets (a directory per set
/// inside, `<set>/...`, unpacked into a directory of the consumer's); a
/// tier no archive names ships in the tree. A name may carry `{v}`, the
/// SDK version read from the SDK project, and `{set}`, the name of the one
/// set inside. The shape is the table's: one archive per tier and per set,
/// two archives (the hosted-runtime tiers in one, the runtime archive sets
/// in the other), or one archive of everything -- each a table edit, the
/// record, the packer and the selfcheck following.
const ASSETS: &[(&str, &[&str], &[&str])] = &[
    ("lamella-rhu-{v}-goldens.tar", &["rt-m7", "rt-m33"], &[]),
    ("lamella-rhu-{v}-runtime-archives.tar", &[], &["rt-m7", "rt-m33"]),
];

/// The package project (the one every RHU package is packed from) and the
/// property whose literal default is the version.
const VERSION_FILE: &str = "src/sdk/Lamella.Rhu.Package/Lamella.Rhu.Package.csproj";
const VERSION_PROPERTY: &str = "RhuSdkVersion";

/// The two runtime archive sets: the world whose tier carries the set's
/// identity file, and the class word.
fn set_worlds() -> [(&'static RtWorld, &'static str); 2] {
    [(&WORLD_M7, "Cortex-M7"), (&WORLD_M33, "Cortex-M33")]
}

/// The set world of a tier key (`rt-m7`), by its index in `set_worlds`.
fn set_index(tier: &str) -> Option<usize> {
    set_worlds().iter().position(|(w, _)| w.tier == format!("goldens/{tier}"))
}

/// A set's world and class word by its tier key.
fn set_of(tier: &str) -> (&'static RtWorld, &'static str) {
    set_worlds()[set_index(tier).unwrap_or(0)]
}

/// A tier's words by its key.
fn tier_desc(tier: &str) -> &'static str {
    TIERS.iter().find(|(k, _)| *k == tier).map(|(_, d)| *d).unwrap_or("a tier")
}

/// The version: the property's literal default in the SDK project.
fn version_of(tree: &dyn Tree) -> Result<String, String> {
    let text = tree
        .read(VERSION_FILE)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .ok_or_else(|| format!("{VERSION_FILE} is absent: the version is read from its {VERSION_PROPERTY}"))?;
    let open = format!("<{VERSION_PROPERTY}");
    let close = format!("</{VERSION_PROPERTY}>");
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix(&open) {
            if let Some(gt) = rest.find('>') {
                if let Some(end) = rest[gt + 1..].find(&close) {
                    let v = rest[gt + 1..gt + 1 + end].trim();
                    if !v.is_empty() && !v.contains('$') && !v.contains(' ') {
                        return Ok(v.to_string());
                    }
                }
            }
        }
    }
    Err(format!("{VERSION_FILE} carries no literal <{VERSION_PROPERTY}>: the version is read from it"))
}

/// The archives' names from the table and the version.
fn asset_names(version: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for (tmpl, _, sets) in ASSETS {
        let mut name = tmpl.replace("{v}", version);
        if name.contains("{set}") {
            if sets.len() != 1 {
                return Err(format!("the archive name {tmpl} takes one set's name; the table gives it {}", sets.len()));
            }
            let (w, _) = set_worlds()[set_index(sets[0]).ok_or_else(|| format!("the archive table names a set of {}, which no world carries", sets[0]))?];
            name = name.replace("{set}", w.archive_set);
        }
        if name.is_empty() || name.contains(' ') || name.contains('/') {
            return Err(format!("the archive name {name} is not a plain file name"));
        }
        if out.contains(&name) {
            return Err(format!("the archive table names {name} twice"));
        }
        out.push(name);
    }
    Ok(out)
}

/// The names the fixture tree's record carries (the selfcheck doctors them).
pub fn fixture_asset_names() -> Vec<String> {
    asset_names("0.0.0-fixture").unwrap_or_default()
}

/// The table's consistency: known tiers and sets, a tier in at most one
/// archive, every set in exactly one.
fn table_check() -> Result<(), String> {
    for (tmpl, tiers, sets) in ASSETS {
        for t in tiers.iter() {
            if !TIERS.iter().any(|(k, _)| k == t) {
                return Err(format!("the archive table names the tier {t} (in {tmpl}), not one of the chain's"));
            }
        }
        for t in sets.iter() {
            if set_index(t).is_none() {
                return Err(format!("the archive table names a set of {t} (in {tmpl}), which no world carries"));
            }
        }
    }
    for (tier, _) in TIERS {
        if ASSETS.iter().filter(|(_, ts, _)| ts.contains(&tier)).count() > 1 {
            return Err(format!("the archive table names goldens/{tier} in two archives"));
        }
    }
    for (w, _) in set_worlds() {
        let tier = w.tier.strip_prefix("goldens/").unwrap_or(w.tier);
        if ASSETS.iter().filter(|(_, _, ss)| ss.contains(&tier)).count() != 1 {
            return Err(format!("the archive table must name the {} set in exactly one archive", w.archive_set));
        }
    }
    Ok(())
}

/// The runtime archive sets as trees, resolved where the hosted-runtime
/// worlds resolve them: the variable, else the first line of the pointer
/// file under `tools/toolchains/`; `None` when neither names a directory.
pub fn resolve_sets(repo: &Path) -> [Option<FsTree>; 2] {
    let one = |w: &RtWorld| -> Option<FsTree> {
        let dir = std::env::var(w.archives_env).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).or_else(|| {
            std::fs::read_to_string(repo.join("tools").join("toolchains").join(w.archives_local))
                .ok()
                .and_then(|t| t.lines().next().map(|l| l.trim().to_string()))
                .filter(|l| !l.is_empty())
        })?;
        let root = PathBuf::from(dir);
        if root.is_dir() { Some(FsTree { root }) } else { None }
    };
    [one(&WORLD_M7), one(&WORLD_M33)]
}

// ---------------------------------------------------------------- the tar

fn octal(field: &mut [u8], value: u64) {
    // `field.len() - 1` octal digits, zero-padded, then NUL.
    let digits = field.len() - 1;
    let text = format!("{value:0width$o}", width = digits);
    field[..digits].copy_from_slice(&text.as_bytes()[text.len() - digits..]);
    field[digits] = 0;
}

/// One member's ustar header.
fn tar_header(path: &str, size: u64) -> Result<[u8; 512], String> {
    if path.len() > 100 || path.is_empty() {
        return Err(format!("a tar member's path must be 1 to 100 bytes: {path}"));
    }
    if size >= 1 << 33 {
        return Err(format!("a tar member's size must fit eleven octal digits: {path} is {size} bytes"));
    }
    let mut h = [0u8; 512];
    h[..path.len()].copy_from_slice(path.as_bytes());
    octal(&mut h[100..108], 0o644);
    octal(&mut h[108..116], 0);
    octal(&mut h[116..124], 0);
    octal(&mut h[124..136], size);
    octal(&mut h[136..148], 0);
    h[148..156].copy_from_slice(b"        ");
    h[156] = b'0';
    h[257..263].copy_from_slice(b"ustar\0");
    h[263..265].copy_from_slice(b"00");
    // The owner names and the device numbers stay NUL for a regular file,
    // as GNU tar writes them in this format.
    let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
    let check = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(check.as_bytes());
    Ok(h)
}

/// The deterministic archive of `members` (path, bytes): sorted bytewise
/// by path, each member a header block and its data padded to the block.
pub fn tar(members: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    let mut sorted: Vec<&(String, Vec<u8>)> = members.iter().collect();
    sorted.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
    let mut out = Vec::new();
    for pair in sorted.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(format!("a tar member's path repeats: {}", pair[0].0));
        }
    }
    for (path, data) in sorted {
        out.extend_from_slice(&tar_header(path, data.len() as u64)?);
        out.extend_from_slice(data);
        let pad = (512 - data.len() % 512) % 512;
        out.extend(std::iter::repeat_n(0u8, pad));
    }
    out.extend(std::iter::repeat_n(0u8, 1024));
    Ok(out)
}

// -------------------------------------------------------------- the words

/// A small count in words.
fn count_word(n: usize) -> String {
    match n {
        1 => "one".to_string(),
        2 => "two".to_string(),
        3 => "three".to_string(),
        4 => "four".to_string(),
        5 => "five".to_string(),
        _ => n.to_string(),
    }
}

/// `a`; `a and b`; `a, b and c`.
fn list_words(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// The first letter upper-cased.
fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Greedy word wrap at `width` columns; a word longer than the width
/// stands on its own line.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// A paragraph of the record: wrapped at 76 columns, a blank line after.
fn para(s: &mut String, text: &str) {
    for l in wrap(text, 76) {
        s.push_str(&l);
        s.push('\n');
    }
    s.push('\n');
}

/// A paragraph of an archive's list: every line a `#` comment, a `#` line after.
fn comment(lines: &mut Vec<String>, text: &str) {
    for l in wrap(text, 70) {
        lines.push(format!("# {l}"));
    }
    lines.push("#".to_string());
}

// -------------------------------------------------------------- the model

/// A tier as the tree holds it.
pub struct TierFacts {
    pub manifest_sha: String,
    pub files: usize,
    pub bytes: u64,
}

/// An archive as the tree (and, for a set, the set directory) yields it.
pub struct AssetFacts {
    pub sha: String,
    pub bytes: u64,
    /// (the member's path as unpacked, its sha256), in the archive's
    /// order; the first is the archive's own `MANIFEST.txt`.
    pub members: Vec<(String, String)>,
    /// The archive's `MANIFEST.txt`, as the archive carries it.
    pub manifest: String,
    /// The archive's bytes.
    pub archive: Vec<u8>,
}

/// One archive as the tree and the set directories yield it.
pub struct AssetModel {
    pub name: String,
    /// The tiers inside (a directory per tier) and the sets inside (a
    /// directory per set), by their tier keys.
    pub tiers: Vec<&'static str>,
    pub sets: Vec<&'static str>,
    /// The set members the identity files expect, (path, sha256) sorted
    /// bytewise, when every set file is in the tree.
    pub listed: Option<Vec<(String, String)>>,
    /// The archive rebuilt, when every tier directory and set directory is here.
    pub facts: Option<AssetFacts>,
    /// What is missing when it is not rebuilt, in words.
    pub missing: Vec<String>,
}

/// What the tree holds of the record's subjects: `None` where a tier
/// directory is absent; an archive's `facts` `None` where a part of it is.
pub struct Model {
    pub version: String,
    /// The record's notice lines, read from the first tier's manifest; `None`
    /// when that tier is absent.
    pub notice: Option<Vec<String>>,
    pub tiers: Vec<(&'static str, Option<TierFacts>)>,
    pub assets: Vec<AssetModel>,
}

/// The record's notice: the paragraph after the title of the first tier's
/// manifest. The tiers' manifests carry the project's notice; the harness
/// carries none of its own, so the record copies theirs.
fn notice_of(tree: &dyn Tree) -> Option<Vec<String>> {
    let path = format!("goldens/{}/MANIFEST.md", TIERS[0].0);
    let text = tree.read(&path).map(|b| String::from_utf8_lossy(&b).into_owned())?;
    let mut paragraphs = text.split("\n\n");
    let _title = paragraphs.next()?;
    let block: Vec<String> = paragraphs.next()?.lines().map(str::to_string).filter(|l| !l.trim().is_empty()).collect();
    if block.is_empty() || block.len() > 3 { None } else { Some(block) }
}

fn tier_dir_members(tree: &dyn Tree, tier: &str) -> Option<Vec<(String, Vec<u8>)>> {
    let dir = format!("goldens/{tier}");
    let names = tree.files(&dir)?;
    let mut members = Vec::new();
    for n in names {
        let path = format!("{dir}/{n}");
        let data = tree.read(&path)?;
        members.push((path, data));
    }
    Some(members)
}

/// The list an archive carries at its root: what the archive holds and
/// where it is unpacked, the one command that checks the unpacked files, then
/// every other member with its size, then every other member's sha256 and
/// path in the shape `sha256sum -c` reads. Every prose line starts with
/// `#`, which `sha256sum -c` skips, so the command reads the file itself.
fn manifest_txt(name: &str, tiers: &[&str], sets: &[&str], members: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = members.iter().collect();
    sorted.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
    let mut lines: Vec<String> = vec![format!("# {name}"), "#".to_string()];
    if !tiers.is_empty() {
        let names: Vec<String> = tiers.iter().map(|t| format!("goldens/{t} ({})", tier_desc(t))).collect();
        let mut s = format!(
            "This archive holds {} of the {} frozen tiers of expected outputs that the verification chain of Lamella RHU compares its results with, each in its own directory: {}.",
            count_word(tiers.len()),
            count_word(TIERS.len()),
            list_words(&names)
        );
        if tiers.len() < TIERS.len() {
            s.push_str(" The other tiers are in the repository under goldens/.");
        }
        comment(&mut lines, &s);
    }
    if !sets.is_empty() {
        let names: Vec<String> = sets
            .iter()
            .map(|t| {
                let (w, class) = set_of(t);
                format!("{} ({class})", w.archive_set)
            })
            .collect();
        let records: Vec<String> = sets
            .iter()
            .map(|t| {
                let (w, _) = set_of(t);
                format!("{}/{}", w.tier, w.set_file)
            })
            .collect();
        comment(
            &mut lines,
            &format!(
                "This archive {} {} of .NET runtime libraries that Lamella RHU built for its processor classes, each in its own directory: {}. The hosted-runtime tiers are rebuilt against these libraries. How each set was built is recorded beside the tier that uses it, in {}.",
                if tiers.is_empty() { "holds" } else { "also holds" },
                if sets.len() == 1 { "one set".to_string() } else { format!("{} sets", count_word(sets.len())) },
                list_words(&names),
                list_words(&records)
            ),
        );
    }
    let (place, from) = if tiers.is_empty() {
        ("into a directory of your own", "that directory")
    } else {
        ("at the root of a checkout of the repository", "that root")
    };
    comment(&mut lines, &format!("Unpack this archive {place}. Then check every unpacked file against this list, from {from}:"));
    lines.push(format!("#     sha256sum -c {ARCHIVE_MANIFEST}"));
    lines.push("#".to_string());
    for t in sets {
        let (w, class) = set_of(t);
        let base = if tiers.is_empty() { "the directory you unpacked into" } else { "the repository root" };
        comment(
            &mut lines,
            &format!(
                "Name the {class} set's directory for the verification chain: put its path ({base}, then {}) into tools/toolchains/{}, or set {} to it.",
                w.archive_set, w.archives_local, w.archives_env
            ),
        );
    }
    comment(&mut lines, "The members and their sizes in bytes:");
    for (p, d) in &sorted {
        lines.push(format!("# {:>10}  {p}", d.len()));
    }
    lines.push("#".to_string());
    comment(&mut lines, "Each line below is a member's SHA256 digest, two spaces and its path. The lines above, which start with a number sign, are comments that sha256sum skips.");
    for (p, d) in &sorted {
        lines.push(format!("{}  {p}", sha256_hex(d)));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The archive of `members` with its `MANIFEST.txt` generated and added.
fn asset_facts(name: &str, tiers: &[&str], sets: &[&str], members: Vec<(String, Vec<u8>)>) -> Result<AssetFacts, String> {
    if members.iter().any(|(p, _)| p == ARCHIVE_MANIFEST) {
        return Err(format!("{name}: a member is named {ARCHIVE_MANIFEST}, the archive's own list"));
    }
    let manifest = manifest_txt(name, tiers, sets, &members);
    let mut all = members;
    all.push((ARCHIVE_MANIFEST.to_string(), manifest.clone().into_bytes()));
    let archive = tar(&all)?;
    all.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
    Ok(AssetFacts {
        sha: sha256_hex(&archive),
        bytes: archive.len() as u64,
        members: all.iter().map(|(p, d)| (p.clone(), sha256_hex(d))).collect(),
        manifest,
        archive,
    })
}

/// The set file's `archive <name> <size> <sha256>` lines.
fn set_lines(text: &str) -> Result<Vec<(String, u64, String)>, String> {
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter_map(|l| l.strip_prefix("archive ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() != 3 || !is_sha(f[2]) {
            return Err(format!("a set file's archive line is not <name> <size> <sha256>: {line}"));
        }
        let size = f[1].parse::<u64>().map_err(|_| format!("a set file's archive size is not a number: {line}"))?;
        out.push((f[0].to_string(), size, f[2].to_string()));
    }
    if out.is_empty() {
        return Err("the set file names no archive".to_string());
    }
    Ok(out)
}

/// The model of `tree` with the two set directories where given.
pub fn model(tree: &dyn Tree, sets: [Option<&dyn Tree>; 2]) -> Result<Model, String> {
    table_check()?;
    let version = version_of(tree)?;
    let notice = notice_of(tree);
    let names = asset_names(&version)?;
    let mut tiers = Vec::new();
    let mut tier_members: BTreeMap<&'static str, Vec<(String, Vec<u8>)>> = BTreeMap::new();
    for (tier, _) in TIERS {
        let facts = match tier_dir_members(tree, tier) {
            None => None,
            Some(members) => {
                let manifest = members
                    .iter()
                    .find(|(p, _)| p == &format!("goldens/{tier}/MANIFEST.md"))
                    .ok_or_else(|| format!("goldens/{tier} carries no MANIFEST.md"))?;
                let facts = TierFacts {
                    manifest_sha: sha256_hex(&manifest.1),
                    files: members.len(),
                    bytes: members.iter().map(|(_, d)| d.len() as u64).sum(),
                };
                tier_members.insert(tier, members);
                Some(facts)
            }
        };
        tiers.push((tier, facts));
    }
    let mut assets = Vec::new();
    for (i, (_, a_tiers, a_sets)) in ASSETS.iter().enumerate() {
        let mut listed: Option<Vec<(String, String)>> = Some(Vec::new());
        let mut members: Option<Vec<(String, Vec<u8>)>> = Some(Vec::new());
        let mut missing = Vec::new();
        for t in a_tiers.iter() {
            match tier_members.get(t) {
                Some(m) => {
                    if let Some(ms) = members.as_mut() {
                        ms.extend(m.iter().cloned());
                    }
                }
                None => {
                    members = None;
                    missing.push(format!("goldens/{t} absent from this tree"));
                }
            }
        }
        for t in a_sets.iter() {
            let si = set_index(t).unwrap_or(0);
            let (w, _) = set_worlds()[si];
            match tree.read(&format!("{}/{}", w.tier, w.set_file)).map(|b| String::from_utf8_lossy(&b).into_owned()) {
                None => {
                    listed = None;
                    members = None;
                    missing.push(format!("the {} set's identity file {}/{} absent from this tree", w.archive_set, w.tier, w.set_file));
                }
                Some(text) => {
                    let lines = set_lines(&text)?;
                    if let Some(l) = listed.as_mut() {
                        l.extend(lines.iter().map(|(n, _, s)| (format!("{}/{n}", w.archive_set), s.clone())));
                    }
                    match (sets[si], members.as_mut()) {
                        (Some(dir), Some(ms)) => {
                            for (n, size, sha) in &lines {
                                let data = dir.read(n).ok_or_else(|| format!("the {} set has no archive {n}", w.archive_set))?;
                                if data.len() as u64 != *size || sha256_hex(&data) != *sha {
                                    return Err(format!(
                                        "the {} set's {n} does not tie the set file ({} bytes, sha256 {}; the file records {size} and {sha})",
                                        w.archive_set,
                                        data.len(),
                                        sha256_hex(&data)
                                    ));
                                }
                                ms.push((format!("{}/{n}", w.archive_set), data));
                            }
                        }
                        (None, _) => {
                            members = None;
                            missing.push(format!(
                                "the {} set directory not reachable here (set {} or write tools/toolchains/{})",
                                w.archive_set, w.archives_env, w.archives_local
                            ));
                        }
                        (Some(_), None) => {}
                    }
                }
            }
        }
        if let Some(l) = listed.as_mut() {
            l.sort();
        }
        let facts = match members {
            Some(ms) => Some(asset_facts(&names[i], a_tiers, a_sets, ms)?),
            None => None,
        };
        assets.push(AssetModel { name: names[i].clone(), tiers: a_tiers.to_vec(), sets: a_sets.to_vec(), listed, facts, missing });
    }
    Ok(Model { version, notice, tiers, assets })
}

/// The archive a tier ships in, by the table.
fn ship_archive<'a>(m: &'a Model, tier: &str) -> Option<&'a AssetModel> {
    m.assets.iter().find(|a| a.tiers.contains(&tier))
}

/// Where a tier is kept, in the record's words.
fn ship_word(m: &Model, tier: &str) -> String {
    match ship_archive(m, tier) {
        None => "in this directory".to_string(),
        Some(a) => format!("in the release archive `{}`", a.name),
    }
}

/// An archive's line in the record: what it holds and where it goes.
fn asset_bullet(a: &AssetModel) -> String {
    let mut parts = Vec::new();
    if !a.tiers.is_empty() {
        parts.push(format!(
            "the {} {}, a directory per tier",
            if a.tiers.len() == 1 { "tier" } else { "tiers" },
            list_words(&a.tiers.iter().map(|t| format!("`goldens/{t}`")).collect::<Vec<_>>())
        ));
    }
    if !a.sets.is_empty() {
        parts.push(format!(
            "the runtime library {} {}, a directory per set",
            if a.sets.len() == 1 { "set" } else { "sets" },
            list_words(
                &a.sets
                    .iter()
                    .map(|t| {
                        let (w, class) = set_of(t);
                        format!("`{}` ({class})", w.archive_set)
                    })
                    .collect::<Vec<_>>()
            )
        ));
    }
    let place = if a.tiers.is_empty() { "into a directory of your own" } else { "at the repository root" };
    format!("`{}` holds {}. It is unpacked {place}.", a.name, parts.join(", and "))
}

// ------------------------------------------------------------- the record

const TIERS_HEAD: &str = "## The tiers\n\n| Tier | Manifest | Files | Bytes | Where |\n|---|---|---|---|---|\n";
const SHAS_HEAD: &str = "| Manifest | SHA256 |\n|---|---|\n";
const ARCHIVES_HEADING: &str = "## The release archives";
const ARCHIVES_HEAD: &str = "| Archive | Members | Bytes | SHA256 |\n|---|---|---|---|\n";

/// The record's opening: the title, the notice, what the goldens are, what
/// the archives hold, how a download is verified.
fn head(m: &Model) -> Result<String, String> {
    let notice = m.notice.as_ref().ok_or_else(|| format!("goldens/{}/MANIFEST.md carries no notice paragraph after its title: the record's notice is read from it", TIERS[0].0))?;
    let mut s = String::from("# The goldens and the release archives\n\n");
    for l in notice {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str("\n## What the goldens are\n\n");
    para(&mut s, "The verification chain compares what it builds and runs with frozen expected outputs, the goldens. They are kept under this directory in five tiers, one directory per tier. Each tier has its own MANIFEST.md naming every file of the tier with its SHA256 digest, and the gate that uses the tier checks that manifest against a digest kept in the harness before its first case.");
    let in_tree: Vec<String> = TIERS.iter().filter(|(k, _)| ship_archive(m, k).is_none()).map(|(_, d)| d.to_string()).collect();
    let archived: Vec<String> = TIERS.iter().filter(|(k, _)| ship_archive(m, k).is_some()).map(|(_, d)| d.to_string()).collect();
    let mut t = String::new();
    if !in_tree.is_empty() {
        t.push_str(&format!("{} {} in this directory.", cap(&list_words(&in_tree)), if in_tree.len() == 1 { "is" } else { "are" }));
    }
    if !archived.is_empty() {
        let verb = if archived.len() == 1 { "is" } else { "are" };
        t.push_str(&format!(
            " {} {verb} larger than the source archive allows and {verb} in the release's archives instead, together with the runtime libraries the hosted-runtime tiers are rebuilt against.",
            cap(&list_words(&archived))
        ));
    }
    para(&mut s, t.trim());
    s.push_str("## What the release archives hold\n\n");
    let n = m.assets.len();
    para(&mut s, &format!("Each release carries {} {} and a SHA256SUMS file beside {}.", count_word(n), if n == 1 { "archive" } else { "archives" }, if n == 1 { "it" } else { "them" }));
    for a in &m.assets {
        let lines = wrap(&asset_bullet(a), 74);
        for (i, l) in lines.iter().enumerate() {
            s.push_str(if i == 0 { "- " } else { "  " });
            s.push_str(l);
            s.push('\n');
        }
    }
    s.push('\n');
    para(&mut s, "Every archive carries its own MANIFEST.txt at its root: what the archive holds, every member with its size and SHA256 digest, and the one command that checks the unpacked files. The sections below repeat each archive's list, so a checkout verifies a download against the revision it holds.");
    s.push_str("## How to verify a download\n\n");
    s.push_str("1. Check the downloaded archives: run `sha256sum -c SHA256SUMS` beside them.\n   Their digests are in the archive table below.\n");
    s.push_str("2. Unpack each archive where its line above says.\n");
    s.push_str("3. Check the unpacked files: run `sha256sum -c MANIFEST.txt` where you\n   unpacked each archive. Then name each runtime library set's directory\n   for the verification chain, as that archive's MANIFEST.txt says.\n\n");
    Ok(s)
}

const TAIL: &str = "## How the archives are made

Each archive is a POSIX ustar archive written by the harness (`rhu-gate
release pack`): the members sorted bytewise by path, mode 0644, owner and
group 0, modification time 0, no owner names, no directory entries. GNU tar
writes the same bytes from the unpacked members:

    tar --format=ustar -b 1 --owner=0 --group=0 --numeric-owner --mtime=@0 --mode=u=rw,go=r -cf <archive> <the members in the order above>

The same tree writes the same bytes on every host. The harness's selfcheck
rebuilds the archives from the tree and refuses a record that does not
match them. This record is written by `rhu-gate release manifest` and is
regenerated in the same commit as any change of a tier.
";

fn fenced(lines: &[String]) -> String {
    let mut s = String::from("```\n");
    for l in lines {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str("```\n");
    s
}

/// The record's text for a model every part of which the tree holds.
pub fn render(m: &Model) -> Result<String, String> {
    let mut s = head(m)?;
    s.push_str(TIERS_HEAD);
    let mut shas = String::new();
    for (tier, desc) in TIERS {
        let facts = m.tiers.iter().find(|(t, _)| *t == tier).and_then(|(_, f)| f.as_ref()).ok_or_else(|| format!("goldens/{tier} is absent: the record is written from a whole tree"))?;
        s.push_str(&format!("| {desc} | `goldens/{tier}/MANIFEST.md` | {} | {} | {} |\n", facts.files, facts.bytes, ship_word(m, tier)));
        shas.push_str(&format!("| `goldens/{tier}/MANIFEST.md` | `{}` |\n", facts.manifest_sha));
    }
    s.push('\n');
    s.push_str(SHAS_HEAD);
    s.push_str(&shas);
    s.push('\n');
    s.push_str(ARCHIVES_HEADING);
    s.push_str("\n\n");
    s.push_str(ARCHIVES_HEAD);
    let mut blocks = String::new();
    for a in &m.assets {
        let facts = a.facts.as_ref().ok_or_else(|| format!("{}: {}: the record is written from a whole tree with every set directory", a.name, a.missing.join("; ")))?;
        s.push_str(&format!("| `{}` | {} | {} | `{}` |\n", a.name, facts.members.len(), facts.bytes, facts.sha));
        blocks.push_str(&format!("\n### {}\n\n", a.name));
        let mut p = String::new();
        para(&mut p, &format!(
            "{} members, {} bytes. The block lists every member in the archive's order with its SHA256 digest. Its first line is the archive's own MANIFEST.txt; the rest is that file's text, as the archive carries it, so a copy of the block checks the unpacked files the same way.",
            facts.members.len(),
            facts.bytes
        ));
        blocks.push_str(&p);
        let mut lines = vec![format!("{}  {ARCHIVE_MANIFEST}", sha256_hex(facts.manifest.as_bytes()))];
        lines.extend(facts.manifest.lines().map(str::to_string));
        blocks.push_str(&fenced(&lines));
    }
    s.push_str(&blocks);
    s.push('\n');
    s.push_str(TAIL);
    Ok(s)
}

// ------------------------------------------------------------ the checker

fn is_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The record as parsed: the tiers (manifest path, sha, files, bytes,
/// ship word) with their words, the archives (name, sha, members, bytes),
/// each archive's members in order and its copy of `MANIFEST.txt`.
pub struct Parsed {
    pub tiers: Vec<(String, String, usize, u64, String)>,
    pub descs: BTreeMap<String, String>,
    pub assets: Vec<(String, String, usize, u64)>,
    pub members: BTreeMap<String, Vec<(String, String)>>,
    pub manifests: BTreeMap<String, String>,
}

/// A table row's cells, trimmed; `None` for a line that is not a row or
/// is the separator row.
fn cells(line: &str) -> Option<Vec<&str>> {
    let inner = line.strip_prefix('|')?.strip_suffix('|')?;
    let c: Vec<&str> = inner.split('|').map(str::trim).collect();
    if c.iter().all(|x| !x.is_empty() && x.bytes().all(|b| b == b'-')) {
        return None;
    }
    Some(c)
}

fn ticked(cell: &str) -> Option<&str> {
    cell.strip_prefix('`')?.strip_suffix('`')
}

fn sum_line(l: &str) -> Result<(String, String), String> {
    let (sha, name) = l.split_once("  ").ok_or_else(|| format!("a digest line is not <sha256>  <path>: {l}"))?;
    if !is_sha(sha) || name.is_empty() || name.contains(' ') {
        return Err(format!("a digest line is not <sha256>  <path>: {l}"));
    }
    Ok((name.to_string(), sha.to_string()))
}

/// The record's text parsed, or the first malformation.
pub fn parse(text: &str) -> Result<Parsed, String> {
    let lines: Vec<&str> = text.lines().collect();
    if !text.ends_with('\n') || text.contains('\r') {
        return Err("the record is not LF-ended text".to_string());
    }
    if let Some(bad) = text.bytes().find(|b| *b > 0x7F) {
        return Err(format!("the record carries a non-ASCII byte 0x{bad:02x}"));
    }
    let mut facts: BTreeMap<String, (String, usize, u64, String)> = BTreeMap::new();
    let mut shas: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut assets = Vec::new();
    let mut members = BTreeMap::new();
    let mut manifests = BTreeMap::new();
    let mut i = 0;
    let mut in_assets = false;
    let mut heading: Option<String> = None;
    while i < lines.len() {
        let l = lines[i];
        if l.starts_with("## ") {
            in_assets = l == ARCHIVES_HEADING;
            heading = None;
        } else if let Some(name) = l.strip_prefix("### ") {
            if !in_assets {
                return Err(format!("a heading stands outside the archives section: {l}"));
            }
            heading = Some(name.to_string());
        } else if l == "```" {
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && lines[end] != "```" {
                end += 1;
            }
            if end >= lines.len() {
                return Err("a fenced block is not closed".to_string());
            }
            let name = match &heading {
                Some(name) if in_assets => name.clone(),
                _ => return Err("a fenced block stands outside an archive's section".to_string()),
            };
            let block = &lines[start..end];
            let first = block.first().copied().unwrap_or("");
            let (p, sha) = sum_line(first)?;
            if p != ARCHIVE_MANIFEST {
                return Err(format!("{name}: the block's first line is not {ARCHIVE_MANIFEST}'s digest: {first}"));
            }
            let mut list = vec![(p, sha)];
            let mut copy = String::new();
            for l in &block[1..] {
                copy.push_str(l);
                copy.push('\n');
                if !l.starts_with('#') {
                    list.push(sum_line(l)?);
                }
            }
            if members.insert(name.clone(), list).is_some() {
                return Err(format!("the archive {name} has two member blocks"));
            }
            manifests.insert(name, copy);
            i = end;
        } else if let Some(c) = cells(l) {
            if in_assets {
                if c.len() == 4 && ticked(c[0]).is_some() {
                    let name = ticked(c[0]).unwrap_or_default();
                    let sha = ticked(c[3]).filter(|s| is_sha(s)).ok_or_else(|| format!("an archive row's sha256 is not sixty-four hex digits: {l}"))?;
                    let n = c[1].parse::<usize>().map_err(|_| format!("an archive row's member count is not a number: {l}"))?;
                    let b = c[2].parse::<u64>().map_err(|_| format!("an archive row's byte count is not a number: {l}"))?;
                    assets.push((name.to_string(), sha.to_string(), n, b));
                } else if c.iter().any(|x| ticked(x).is_some()) {
                    return Err(format!("a table row of the archives section is not <archive> | <members> | <bytes> | <sha256>: {l}"));
                }
            } else if c.len() == 5 && ticked(c[1]).is_some() {
                let path = ticked(c[1]).unwrap_or_default().to_string();
                let files = c[2].parse::<usize>().map_err(|_| format!("a tier row's file count is not a number: {l}"))?;
                let bytes = c[3].parse::<u64>().map_err(|_| format!("a tier row's byte count is not a number: {l}"))?;
                if facts.insert(path.clone(), (c[0].to_string(), files, bytes, c[4].to_string())).is_some() {
                    return Err(format!("the record has two rows for {path}"));
                }
                order.push(path);
            } else if c.len() == 2 && ticked(c[0]).is_some() {
                let path = ticked(c[0]).unwrap_or_default().to_string();
                let sha = ticked(c[1]).filter(|s| is_sha(s)).ok_or_else(|| format!("a manifest row's sha256 is not sixty-four hex digits: {l}"))?;
                if shas.insert(path.clone(), sha.to_string()).is_some() {
                    return Err(format!("the record has two sha256 rows for {path}"));
                }
            } else if c.iter().any(|x| ticked(x).is_some()) {
                return Err(format!("a table row of the tiers section is not one the record writes: {l}"));
            }
        }
        i += 1;
    }
    let mut tiers = Vec::new();
    let mut descs = BTreeMap::new();
    for path in order {
        let (desc, files, bytes, ship) = facts.remove(&path).unwrap_or_default();
        let sha = shas.remove(&path).ok_or_else(|| format!("the record has no sha256 row for {path}"))?;
        descs.insert(path.clone(), desc);
        tiers.push((path, sha, files, bytes, ship));
    }
    if let Some((path, _)) = shas.iter().next() {
        return Err(format!("the record has a sha256 row for {path} and no tier row"));
    }
    if tiers.is_empty() || assets.is_empty() {
        return Err("the record names no tier or no archive".to_string());
    }
    Ok(Parsed { tiers, descs, assets, members, manifests })
}

/// The committed record against the model: every record whose subject
/// the tree holds verified, a subject the tree lacks declared by name in
/// the returned lines; the first disagreement is the error.
pub fn check(text: &str, m: &Model) -> Result<Vec<String>, String> {
    let p = parse(text)?;
    let mut lines = Vec::new();
    if p.tiers.len() != TIERS.len() {
        return Err(format!("the record names {} tiers, the chain has {}", p.tiers.len(), TIERS.len()));
    }
    for (tier, desc) in TIERS {
        let path = format!("goldens/{tier}/MANIFEST.md");
        let (_, sha, files, bytes, ship) = p.tiers.iter().find(|(pp, ..)| *pp == path).ok_or_else(|| format!("the record has no row for {path}"))?;
        let want_ship = ship_word(m, tier);
        if *ship != want_ship {
            return Err(format!("{path}: the record says it ships {ship}; the table says {want_ship}"));
        }
        if p.descs.get(&path).map(String::as_str) != Some(desc) {
            return Err(format!("{path}: the record's words for the tier are not the chain's ({desc})"));
        }
        match m.tiers.iter().find(|(t, _)| *t == tier).and_then(|(_, f)| f.as_ref()) {
            None => lines.push(format!("goldens/{tier}: absent from this tree (kept {want_ship}); its records declared, not verified")),
            Some(f) => {
                if f.manifest_sha != *sha {
                    return Err(format!("{path}: the tree's sha256 {} is not the record's {sha} (a re-freeze without the record: run `rhu-gate release manifest` in the same commit)", f.manifest_sha));
                }
                if f.files != *files || f.bytes != *bytes {
                    return Err(format!("goldens/{tier}: the tree holds {} files, {} bytes; the record says {files} files, {bytes} bytes", f.files, f.bytes));
                }
                lines.push(format!("goldens/{tier}: the manifest's sha256, {files} files and {bytes} bytes tie the record"));
            }
        }
    }
    let expected: Vec<String> = m.assets.iter().map(|a| a.name.clone()).collect();
    let named: Vec<String> = p.assets.iter().map(|(n, ..)| n.clone()).collect();
    if named != expected {
        return Err(format!("the record names the archives [{}]; the table names [{}]", named.join(", "), expected.join(", ")));
    }
    for name in &expected {
        if !p.members.contains_key(name) {
            return Err(format!("the archive {name} has no member block"));
        }
    }
    if p.members.len() != expected.len() {
        return Err("the record has a member block for an archive the release has not".to_string());
    }
    for a in &m.assets {
        let (_, rec_sha, rec_n, rec_bytes) = p.assets.iter().find(|(n, ..)| *n == a.name).cloned().unwrap_or_default();
        let rec = &p.members[&a.name];
        let copy = &p.manifests[&a.name];
        if rec.first().map(|(_, s)| s.as_str()) != Some(sha256_hex(copy.as_bytes()).as_str()) {
            return Err(format!("{}: the record's digest of {ARCHIVE_MANIFEST} is not the digest of the text that follows it", a.name));
        }
        if rec.len() != rec_n {
            return Err(format!("{}: the archive row says {rec_n} members, the block lists {}", a.name, rec.len()));
        }
        if let Some(l) = &a.listed {
            let set_members: Vec<(String, String)> = rec.iter().filter(|(p, _)| !p.starts_with("goldens/") && p != ARCHIVE_MANIFEST).cloned().collect();
            if set_members != *l {
                return Err(format!("{}: the record's set members are not the set files' archive lines", a.name));
            }
        }
        match &a.facts {
            Some(f) => {
                if f.manifest != *copy {
                    return Err(format!("{}: the record's copy of {ARCHIVE_MANIFEST} is not the archive's", a.name));
                }
                if f.members != *rec {
                    return Err(format!("{}: the members rebuilt here ({}) are not the record's ({})", a.name, f.members.len(), rec.len()));
                }
                if f.sha != rec_sha || f.bytes != rec_bytes {
                    return Err(format!("{}: the archive rebuilt here is {} bytes with sha256 {}, the record says {rec_bytes} and {rec_sha}", a.name, f.bytes, f.sha));
                }
                lines.push(format!(
                    "{}: rebuilt from the tree{}, {} members, {} bytes, sha256 ties the record",
                    a.name,
                    if a.sets.is_empty() { "" } else { " and the set directory" },
                    f.members.len(),
                    f.bytes
                ));
            }
            None => lines.push(format!(
                "{}: {}; the archive declared, not rebuilt{}",
                a.name,
                a.missing.join("; "),
                if a.listed.is_some() && !a.sets.is_empty() { " (its set members tie the set file)" } else { "" }
            )),
        }
    }
    Ok(lines)
}

// -------------------------------------------------------------- the verbs

fn sets_as_dyn(sets: &[Option<FsTree>; 2]) -> [Option<&dyn Tree>; 2] {
    [sets[0].as_ref().map(|t| t as &dyn Tree), sets[1].as_ref().map(|t| t as &dyn Tree)]
}

/// `rhu-gate release manifest`: the record written from the whole tree
/// and both set directories.
pub fn manifest(repo: &Path) -> ! {
    let tree = FsTree { root: repo.to_path_buf() };
    let sets = resolve_sets(repo);
    let m = match model(&tree, sets_as_dyn(&sets)) {
        Ok(m) => m,
        Err(e) => {
            println!("RELEASE MANIFEST: REFUSE -- {e}");
            exit(1);
        }
    };
    let text = match render(&m) {
        Ok(t) => t,
        Err(e) => {
            println!("RELEASE MANIFEST: REFUSE -- {e}");
            exit(1);
        }
    };
    let path = repo.join(RECORD.replace('/', std::path::MAIN_SEPARATOR_STR));
    let before = std::fs::read(&path).ok();
    if let Err(e) = std::fs::write(&path, text.as_bytes()) {
        println!("RELEASE MANIFEST: REFUSE -- cannot write {RECORD}: {e}");
        exit(1);
    }
    let moved = match before {
        None => "written (no record before)",
        Some(b) if b == text.as_bytes() => "unchanged (the tree writes the same bytes)",
        Some(_) => "rewritten (the record moved: commit it with the re-freeze)",
    };
    for a in &m.assets {
        if let Some(f) = &a.facts {
            println!("  {}  {} ({} members, {} bytes)", f.sha, a.name, f.members.len(), f.bytes);
        }
    }
    println!("RELEASE MANIFEST: OK ({RECORD} {moved}; {} bytes, sha256 {})", text.len(), sha256_hex(text.as_bytes()));
    exit(0);
}

/// `rhu-gate release pack <out-dir>`: the committed record checked against
/// the tree and both set directories, then the archives and `SHA256SUMS`
/// written into the directory.
pub fn pack(repo: &Path, out: &str) -> ! {
    let tree = FsTree { root: repo.to_path_buf() };
    let sets = resolve_sets(repo);
    let text = match std::fs::read_to_string(repo.join(RECORD.replace('/', std::path::MAIN_SEPARATOR_STR))) {
        Ok(t) => t,
        Err(e) => {
            println!("RELEASE PACK: REFUSE -- {RECORD} unreadable: {e}");
            exit(1);
        }
    };
    let m = match model(&tree, sets_as_dyn(&sets)) {
        Ok(m) => m,
        Err(e) => {
            println!("RELEASE PACK: REFUSE -- {e}");
            exit(1);
        }
    };
    match check(&text, &m) {
        Ok(lines) => {
            for l in &lines {
                println!("  {l}");
            }
        }
        Err(e) => {
            println!("RELEASE PACK: REFUSE -- the record does not tie the tree: {e}");
            exit(1);
        }
    }
    let out_dir = PathBuf::from(out);
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        println!("RELEASE PACK: REFUSE -- cannot create {out}: {e}");
        exit(1);
    }
    let mut sums = String::new();
    for a in &m.assets {
        let Some(f) = &a.facts else {
            println!("RELEASE PACK: REFUSE -- {}: {}: the archives are packed from a whole tree with every set directory", a.name, a.missing.join("; "));
            exit(1);
        };
        if let Err(e) = std::fs::write(out_dir.join(&a.name), &f.archive) {
            println!("RELEASE PACK: REFUSE -- cannot write {} into {out}: {e}", a.name);
            exit(1);
        }
        println!("  {}  {} ({} bytes; equals the record)", f.sha, a.name, f.bytes);
        sums.push_str(&format!("{}  {}\n", f.sha, a.name));
    }
    if let Err(e) = std::fs::write(out_dir.join("SHA256SUMS"), sums.as_bytes()) {
        println!("RELEASE PACK: REFUSE -- cannot write SHA256SUMS into {out}: {e}");
        exit(1);
    }
    println!("RELEASE PACK: OK ({} archives and SHA256SUMS written into {out} for the version {}; every sha256 equals {RECORD})", m.assets.len(), m.version);
    exit(0);
}

// ------------------------------------------------------------ the fixture

/// A fixture tree with the five tiers (two files each), both set files,
/// and the two set trees: the selfcheck's subject for the checker's both
/// directions.
pub fn fixture() -> (MapTree, MapTree, MapTree) {
    let mut t: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    t.insert(
        VERSION_FILE.to_string(),
        format!("<Project>\n  <PropertyGroup>\n    <{VERSION_PROPERTY} Condition=\"'$({VERSION_PROPERTY})' == ''\">0.0.0-fixture</{VERSION_PROPERTY}>\n  </PropertyGroup>\n</Project>\n").into_bytes(),
    );
    for tier in ["gc", "shim", "payload", "rt-m7", "rt-m33"] {
        t.insert(format!("goldens/{tier}/MANIFEST.md"), format!("# {tier} fixture manifest\n\nFixture notice, line one.\nFixture notice, line two.\n").into_bytes());
        t.insert(format!("goldens/{tier}/object.o"), format!("{tier} object bytes\n").into_bytes());
    }
    let mut sets = Vec::new();
    for w in [&WORLD_M7, &WORLD_M33] {
        let a = format!("{} archive one\n", w.archive_set).into_bytes();
        let b = format!("{} archive two\n", w.archive_set).into_bytes();
        let set_file = format!(
            "set-id: {}\narchive libOne.a {} {}\narchive libTwo.a {} {}\narchives-sha256: fixture\n",
            w.archive_set,
            a.len(),
            sha256_hex(&a),
            b.len(),
            sha256_hex(&b)
        );
        t.insert(format!("{}/{}", w.tier, w.set_file), set_file.into_bytes());
        let mut s: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        s.insert("libOne.a".to_string(), a);
        s.insert("libTwo.a".to_string(), b);
        sets.push(MapTree(s));
    }
    let set_m33 = sets.pop().unwrap_or_else(|| MapTree(BTreeMap::new()));
    let set_m7 = sets.pop().unwrap_or_else(|| MapTree(BTreeMap::new()));
    (MapTree(t), set_m7, set_m33)
}

/// The two members whose archive the selfcheck pins: a text file and a
/// binary one block and one byte long, under one directory, given in the
/// order the writer must not keep.
pub fn tar_fixture_members() -> Vec<(String, Vec<u8>)> {
    vec![("fixture/text.txt".to_string(), b"hello\n".to_vec()), ("fixture/bytes.bin".to_string(), vec![7u8; 513])]
}

/// `rhu-gate release fixture-tar <file>`: the pinned fixture's archive
/// written out, so another tar writer's bytes can be compared with it.
pub fn fixture_tar(file: &str) -> ! {
    let bytes = match tar(&tar_fixture_members()) {
        Ok(b) => b,
        Err(e) => {
            println!("RELEASE FIXTURE-TAR: REFUSE -- {e}");
            exit(1);
        }
    };
    if let Err(e) = std::fs::write(file, &bytes) {
        println!("RELEASE FIXTURE-TAR: REFUSE -- cannot write {file}: {e}");
        exit(1);
    }
    println!("RELEASE FIXTURE-TAR: OK ({file}: {} bytes, sha256 {})", bytes.len(), sha256_hex(&bytes));
    exit(0);
}
