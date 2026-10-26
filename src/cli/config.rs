//! .csharpierrc and .editorconfig handling

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::glob::{self, GlobMatcher};
use crate::doc::EndOfLine;
use crate::options::{xml_whitespace_sensitivity_for_path, FileOptions, Formatter};
use crate::xml::XmlWhitespaceSensitivity;

fn path_string(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

// ---- csharpierrc ----------------------------------------------------------------------------

#[derive(Default, Clone)]
struct RawOptions {
    print_width: Option<usize>,
    indent_size: Option<usize>,
    use_tabs: bool,
    xml_whitespace_sensitivity: Option<XmlWhitespaceSensitivity>,
    end_of_line: EndOfLine,
}

pub struct Override {
    options: RawOptions,
    files: String,
    formatter: String,
    matcher: Option<GlobMatcher>,
}

pub struct CSharpierConfig {
    options: RawOptions,
    overrides: Vec<Override>,
    pub directory: PathBuf,
}

fn lower_keys(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.to_lowercase(), lower_keys(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(lower_keys).collect()),
        other => other.clone(),
    }
}

fn raw_options(v: &Value) -> RawOptions {
    let get_usize = |k: &str| v.get(k).and_then(|x| x.as_u64()).map(|x| x as usize);
    RawOptions {
        print_width: get_usize("printwidth"),
        indent_size: get_usize("indentsize"),
        use_tabs: v.get("usetabs").and_then(|x| x.as_bool()).unwrap_or(false),
        xml_whitespace_sensitivity: v
            .get("xmlwhitespacesensitivity")
            .and_then(|x| x.as_str())
            .and_then(|s| match s.to_lowercase().as_str() {
                "strict" => Some(XmlWhitespaceSensitivity::Strict),
                "ignore" => Some(XmlWhitespaceSensitivity::Ignore),
                _ => None,
            }),
        end_of_line: match v.get("endofline").and_then(|x| x.as_str()).map(|s| s.to_lowercase()).as_deref() {
            Some("lf") => EndOfLine::Lf,
            Some("crlf") => EndOfLine::Crlf,
            _ => EndOfLine::Auto,
        },
    }
}

impl CSharpierConfig {
    pub fn parse(content: &str, directory: &Path) -> Result<CSharpierConfig, String> {
        let value: Value = if content.trim_start().starts_with('{') {
            serde_json::from_str(content).map_err(|e| e.to_string())?
        } else {
            serde_yaml::from_str(content).map_err(|e| e.to_string())?
        };
        let value = lower_keys(&value);
        let dir = path_string(directory);
        let overrides = value
            .get("overrides")
            .and_then(|o| o.as_array())
            .map(|a| {
                a.iter()
                    .map(|o| {
                        let files = o.get("files").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        Override {
                            options: raw_options(o),
                            matcher: Some(glob::create(&files, Some(&dir))),
                            files,
                            formatter: o.get("formatter").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(CSharpierConfig {
            options: raw_options(&value),
            overrides,
            directory: directory.to_path_buf(),
        })
    }

    pub fn from_file(path: &Path) -> Result<CSharpierConfig, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(CSharpierConfig {
                options: RawOptions::default(),
                overrides: vec![],
                directory: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            });
        }
        CSharpierConfig::parse(&content, path.parent().unwrap_or(Path::new("")))
    }

    pub fn options_for(&self, file_path: &str) -> Option<FileOptions> {
        let norm = file_path.replace('\\', "/");
        if let Some(o) = self.overrides.iter().rev().find(|o| o.matcher.as_ref().map(|m| m.is_match(&norm)).unwrap_or(false)) {
            let formatter = Formatter::parse(&o.formatter)?;
            let _ = &o.files;
            return Some(create(formatter, file_path, &o.options));
        }
        let formatter = Formatter::from_path(file_path);
        (formatter != Formatter::Unknown).then(|| create(formatter, file_path, &self.options))
    }
}

fn create(formatter: Formatter, file_path: &str, o: &RawOptions) -> FileOptions {
    let mut fo = FileOptions::new(
        formatter,
        o.xml_whitespace_sensitivity.unwrap_or_else(|| xml_whitespace_sensitivity_for_path(file_path)),
    );
    fo.printer.use_tabs = o.use_tabs;
    fo.printer.end_of_line = o.end_of_line;
    if let Some(i) = o.indent_size {
        if i > 0 {
            fo.printer.indent_size = i;
        }
    }
    if let Some(w) = o.print_width {
        fo.printer.width = w;
    }
    fo
}

/// the .csharpierrc file in a directory, if there is one
pub fn find_config_file_in(dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if !name.starts_with(".csharpierrc") {
                return false;
            }
            let ext = &name[".csharpierrc".len()..];
            matches!(ext.to_lowercase().as_str(), "" | ".json" | ".yml" | ".yaml")
                || (ext.is_empty())
        })
        .filter(|p| p.is_file())
        .collect();
    candidates.sort_by_key(|p| p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default());
    candidates.into_iter().next()
}

// ---- editorconfig ---------------------------------------------------------------------------

struct Section {
    matcher: GlobMatcher,
    no_directory_matcher: GlobMatcher,
    keys: std::collections::HashMap<String, String>,
}

struct EditorConfigFile {
    is_root: bool,
    sections: Vec<Section>,
}

fn parse_editorconfig(path: &Path) -> EditorConfigFile {
    let dir = path_string(path.parent().unwrap_or(Path::new("")));
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let mut is_root = false;
    let mut sections: Vec<(String, std::collections::HashMap<String, String>)> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].to_string();
            // duplicate sections are allowed
            sections.push((name, Default::default()));
        } else if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim().to_lowercase(), v.trim().to_string());
            match sections.last_mut() {
                Some((_, keys)) => {
                    keys.insert(k, v);
                }
                None => {
                    if k == "root" && v == "true" {
                        is_root = true;
                    }
                }
            }
        }
    }
    EditorConfigFile {
        is_root,
        sections: sections
            .into_iter()
            .map(|(name, keys)| Section {
                matcher: glob::create(&name, Some(&dir)),
                no_directory_matcher: glob::create(&name, None),
                keys,
            })
            .collect(),
    }
}

pub struct EditorConfigSections {
    sections: Vec<Section>,
}

pub fn find_editorconfig(dir: &Path) -> Option<EditorConfigSections> {
    let mut files = Vec::new();
    let mut found = false;
    let mut cur = Some(dir.to_path_buf());
    while let Some(d) = cur {
        let p = d.join(".editorconfig");
        if p.is_file() {
            found = true;
            let f = parse_editorconfig(&p);
            let root = f.is_root;
            files.push(f);
            if root {
                break;
            }
        }
        cur = d.parent().map(|p| p.to_path_buf());
    }
    if !found {
        return None;
    }
    // farthest file first so closer files win
    let sections = files.into_iter().rev().flat_map(|f| f.sections).collect();
    Some(EditorConfigSections { sections })
}

impl EditorConfigSections {
    pub fn options_for(&self, file_path: &str, ignore_directory: bool) -> Option<FileOptions> {
        let norm = file_path.replace('\\', "/");
        let matching: Vec<&Section> = self
            .sections
            .iter()
            .filter(|s| if ignore_directory { s.no_directory_matcher.is_match(&norm) } else { s.matcher.is_match(&norm) })
            .collect();
        let last = |key: &str| matching.iter().rev().find_map(|s| s.keys.get(key).cloned());

        let formatter_name = last("csharpier_formatter");
        let formatter = match &formatter_name {
            Some(n) => Formatter::parse(n)?,
            None => Formatter::from_path(file_path),
        };
        let mut fo = FileOptions::new(formatter, xml_whitespace_sensitivity_for_path(file_path));

        if let Some(w) = last("max_line_length").and_then(|v| v.parse::<usize>().ok()).filter(|w| *w > 0) {
            fo.printer.width = w;
        }
        let indent_style = last("indent_style").filter(|s| s == "space" || s == "tab");
        if indent_style.as_deref() == Some("tab") {
            fo.printer.use_tabs = true;
        }
        let indent_size = last("indent_size");
        let tab_width = last("tab_width");
        let (resolved_indent, resolved_tab) = if indent_size.as_deref() == Some("tab") {
            let t = tab_width.and_then(|v| v.parse::<usize>().ok());
            (t, t)
        } else {
            let i = indent_size.and_then(|v| v.parse::<usize>().ok());
            (i, tab_width.and_then(|v| v.parse::<usize>().ok()).or(i))
        };
        if fo.printer.use_tabs {
            if let Some(t) = resolved_tab.filter(|t| *t > 0) {
                fo.printer.indent_size = t;
            }
        } else if let Some(i) = resolved_indent.filter(|i| *i > 0) {
            fo.printer.indent_size = i;
        }
        match last("end_of_line").map(|s| s.to_lowercase()).as_deref() {
            Some("lf") => fo.printer.end_of_line = EndOfLine::Lf,
            Some("crlf") => fo.printer.end_of_line = EndOfLine::Crlf,
            _ => {}
        }
        match last("csharpier_xml_whitespace_sensitivity").map(|s| s.to_lowercase()).as_deref() {
            Some("strict") => fo.xml_whitespace_sensitivity = XmlWhitespaceSensitivity::Strict,
            Some("ignore") => fo.xml_whitespace_sensitivity = XmlWhitespaceSensitivity::Ignore,
            _ => {}
        }
        Some(fo)
    }
}

// ---- the provider ---------------------------------------------------------------------------

pub struct OptionsProvider {
    specified_config: Option<CSharpierConfig>,
    specified_editorconfig: Option<EditorConfigSections>,
    warn: Option<Box<dyn Fn(String) + Send + Sync>>,
    warned: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
}

impl OptionsProvider {
    pub fn new(config_path: Option<&Path>) -> Result<OptionsProvider, String> {
        let mut specified_config = None;
        let mut specified_editorconfig = None;
        if let Some(p) = config_path {
            if p.file_name().map(|n| n == ".editorconfig").unwrap_or(false) {
                specified_editorconfig = find_editorconfig(p.parent().unwrap_or(Path::new(".")));
            } else {
                specified_config = Some(CSharpierConfig::from_file(p)?);
            }
        }
        Ok(OptionsProvider {
            specified_config,
            specified_editorconfig,
            warn: None,
            warned: Default::default(),
        })
    }

    /// reports problems with configuration files, like empty ones
    pub fn with_warnings(mut self, warn: impl Fn(String) + Send + Sync + 'static) -> OptionsProvider {
        self.warn = Some(Box::new(warn));
        self
    }

    fn warn_if_empty(&self, cfg: &Path) {
        let empty = std::fs::read_to_string(cfg).map(|c| c.trim().is_empty()).unwrap_or(false);
        if empty && self.warned.lock().unwrap().insert(cfg.to_path_buf()) {
            if let Some(w) = &self.warn {
                w(format!("The configuration file at {} was empty.", cfg.display()));
            }
        }
    }

    pub fn options_for(&self, file_path: &Path) -> Option<FileOptions> {
        let fp = path_string(file_path);
        if let Some(c) = &self.specified_config {
            return c.options_for(&fp);
        }
        if let Some(e) = &self.specified_editorconfig {
            return e.options_for(&fp, true);
        }
        let dir = file_path.parent().unwrap_or(Path::new("."));
        let mut cur = Some(dir.to_path_buf());
        while let Some(d) = cur {
            if d.is_dir() {
                if let Some(cfg) = find_config_file_in(&d) {
                    self.warn_if_empty(&cfg);
                    return match CSharpierConfig::from_file(&cfg) {
                        Ok(c) => c.options_for(&fp),
                        Err(_) => FileOptions::default_for_path(&fp),
                    };
                }
            }
            cur = d.parent().map(|p| p.to_path_buf());
        }
        if let Some(e) = find_editorconfig(dir) {
            return e.options_for(&fp, false);
        }
        FileOptions::default_for_path(&fp)
    }
}
