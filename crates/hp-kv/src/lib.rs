use std::fmt::Write as _;
use std::str::FromStr;

mod encoding;

pub use encoding::{TextEncoding, decode, decode_windows_1252, encode, encode_windows_1252};

#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error("invalid KeyValues input: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Block {
    pub name: String,
    pub props: Vec<(String, String)>,
    pub children: Vec<Block>,
}

impl Block {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), props: Vec::new(), children: Vec::new() }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.props.iter().filter(move |(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn get_parsed<T: FromStr>(&self, key: &str) -> Option<T> {
        self.get(key).and_then(|v| v.trim().parse().ok())
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();
        if let Some(slot) = self.props.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(&key)) {
            slot.1 = value;
        } else {
            self.props.push((key, value));
        }
    }

    pub fn push_prop(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.props.push((key.into(), value.into()));
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        let mut first = None;
        self.props.retain_mut(|(k, v)| {
            if k.eq_ignore_ascii_case(key) {
                if first.is_none() {
                    first = Some(std::mem::take(v));
                }
                false
            } else {
                true
            }
        });
        first
    }

    pub fn child(&self, name: &str) -> Option<&Block> {
        self.children.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn child_mut(&mut self, name: &str) -> Option<&mut Block> {
        self.children.iter_mut().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Block> + 'a {
        self.children.iter().filter(move |c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn push_child(&mut self, child: Block) -> &mut Block {
        self.children.push(child);
        self.children.last_mut().expect("just pushed")
    }

    pub fn to_kv_string(&self) -> String {
        let mut out = String::new();
        write_block(self, 0, &mut out);
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok<'a> {
    Str(&'a str),
    Open,
    Close,
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    unterminated: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        Self { src, bytes: src.as_bytes(), pos: 0, unterminated: false }
    }

    #[inline]
    fn skip_ws_and_comments(&mut self) {
        let b = self.bytes;
        loop {
            while self.pos < b.len() && b[self.pos].is_ascii_whitespace() {
                self.pos += 1;
            }
            if self.pos + 1 < b.len() && b[self.pos] == b'/' && b[self.pos + 1] == b'/' {
                match memchr::memchr(b'\n', &b[self.pos..]) {
                    Some(off) => self.pos += off + 1,
                    None => self.pos = b.len(),
                }
                continue;
            }
            break;
        }
    }

    fn next(&mut self) -> Option<(Tok<'a>, bool)> {
        self.skip_ws_and_comments();
        let b = self.bytes;
        if self.pos >= b.len() {
            return None;
        }
        match b[self.pos] {
            b'{' => {
                self.pos += 1;
                Some((Tok::Open, false))
            }
            b'}' => {
                self.pos += 1;
                Some((Tok::Close, false))
            }
            b'"' => {
                let start = self.pos + 1;
                let end = match memchr::memchr(b'"', &b[start..]) {
                    Some(off) => start + off,
                    None => {
                        self.unterminated = true;
                        b.len()
                    }
                };
                self.pos = (end + 1).min(b.len());
                Some((Tok::Str(&self.src[start..end]), false))
            }
            b'[' => {
                let start = self.pos;
                let end = match memchr::memchr(b']', &b[start..]) {
                    Some(off) => start + off + 1,
                    None => b.len(),
                };
                self.pos = end;
                Some((Tok::Str(&self.src[start..end]), true))
            }
            _ => {
                let start = self.pos;
                while self.pos < b.len() {
                    let c = b[self.pos];
                    if c.is_ascii_whitespace() || c == b'{' || c == b'}' || c == b'"' {
                        break;
                    }
                    if c == b'/' && self.pos + 1 < b.len() && b[self.pos + 1] == b'/' {
                        break;
                    }
                    self.pos += 1;
                }
                Some((Tok::Str(&self.src[start..self.pos]), false))
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParseIssues {
    pub unclosed_blocks: usize,
    pub unterminated_string: bool,
    pub stray_closes: usize,
    pub missing_values: usize,
}

impl ParseIssues {
    pub fn is_clean(&self) -> bool {
        *self == Self::default()
    }

    pub fn is_truncated(&self) -> bool {
        self.unterminated_string || self.unclosed_blocks > 0
    }
}

pub fn parse(src: &str) -> Result<Block, KvError> {
    Ok(parse_with_issues(src).0)
}

pub fn parse_bounded(src: &str, max_depth: usize) -> Result<Block, KvError> {
    let mut lexer = Lexer::new(src);
    let mut depth = 0usize;
    while let Some((token, _)) = lexer.next() {
        match token {
            Tok::Open => {
                depth += 1;
                if depth > max_depth {
                    return Err(KvError::Invalid("nesting limit exceeded".into()));
                }
            }
            Tok::Close => {
                if depth == 0 {
                    return Err(KvError::Invalid("unexpected closing brace".into()));
                }
                depth -= 1;
            }
            Tok::Str(_) => {}
        }
    }
    if depth != 0 || lexer.unterminated {
        return Err(KvError::Invalid("incomplete input".into()));
    }
    let (root, issues) = parse_with_issues(src);
    if !issues.is_clean() {
        return Err(KvError::Invalid("malformed properties".into()));
    }
    Ok(root)
}

pub fn parse_with_issues(src: &str) -> (Block, ParseIssues) {
    let mut lexer = Lexer::new(src);
    let mut stack: Vec<Block> = vec![Block::new("")];
    let mut pending_key: Option<&str> = None;
    let mut issues = ParseIssues::default();

    while let Some((tok, is_cond)) = lexer.next() {
        if is_cond {
            continue;
        }
        match tok {
            Tok::Open => {
                let name = pending_key.take().unwrap_or("");
                stack.push(Block::new(name));
            }
            Tok::Close => {
                if pending_key.take().is_some() {
                    issues.missing_values += 1;
                }
                if stack.len() > 1 {
                    let done = stack.pop().expect("len > 1");
                    stack.last_mut().expect("root").children.push(done);
                } else {
                    issues.stray_closes += 1;
                }
            }
            Tok::Str(s) => match pending_key.take() {
                None => pending_key = Some(s),
                Some(key) => {
                    stack.last_mut().expect("root").props.push((key.to_owned(), s.to_owned()));
                }
            },
        }
    }
    if pending_key.is_some() {
        issues.missing_values += 1;
    }
    issues.unterminated_string = lexer.unterminated;

    issues.unclosed_blocks = stack.len() - 1;
    while stack.len() > 1 {
        let done = stack.pop().expect("len > 1");
        stack.last_mut().expect("root").children.push(done);
    }
    (stack.pop().expect("root"), issues)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockRef<'a> {
    pub name: &'a str,
    pub props: Vec<(&'a str, &'a str)>,
    pub children: Vec<BlockRef<'a>>,
}

impl<'a> BlockRef<'a> {
    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| *v)
    }

    pub fn to_block(&self) -> Block {
        Block {
            name: self.name.to_owned(),
            props: self.props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            children: self.children.iter().map(BlockRef::to_block).collect(),
        }
    }
}

impl Block {
    pub fn as_ref(&self) -> BlockRef<'_> {
        BlockRef {
            name: &self.name,
            props: self.props.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect(),
            children: self.children.iter().map(Block::as_ref).collect(),
        }
    }
}

pub fn parse_ref(src: &str) -> BlockRef<'_> {
    let mut lexer = Lexer::new(src);
    let mut stack: Vec<BlockRef<'_>> = vec![BlockRef::default()];
    let mut pending_key: Option<&str> = None;
    while let Some((tok, is_cond)) = lexer.next() {
        if is_cond {
            continue;
        }
        match tok {
            Tok::Open => {
                let name = pending_key.take().unwrap_or("");
                stack.push(BlockRef { name, ..BlockRef::default() });
            }
            Tok::Close => {
                pending_key = None;
                if stack.len() > 1 {
                    let done = stack.pop().expect("len > 1");
                    stack.last_mut().expect("root").children.push(done);
                }
            }
            Tok::Str(s) => match pending_key.take() {
                None => pending_key = Some(s),
                Some(key) => stack.last_mut().expect("root").props.push((key, s)),
            },
        }
    }
    while stack.len() > 1 {
        let done = stack.pop().expect("len > 1");
        stack.last_mut().expect("root").children.push(done);
    }
    stack.pop().expect("root")
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Block, KvError> {
    let (text, _) = decode(bytes);
    parse(&text)
}

#[inline]
fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push('\t');
    }
}

#[inline]
fn push_quoted(out: &mut String, s: &str) {
    out.push('"');
    if s.contains('"') {
        out.push_str(&s.replace('"', "'"));
    } else {
        out.push_str(s);
    }
    out.push('"');
}

pub fn write_block(b: &Block, indent: usize, out: &mut String) {
    push_indent(out, indent);
    out.push_str(&b.name);
    out.push('\n');
    push_indent(out, indent);
    out.push_str("{\n");
    write_contents(b, indent + 1, out);
    push_indent(out, indent);
    out.push_str("}\n");
}

pub fn write_contents(b: &Block, indent: usize, out: &mut String) {
    for (k, v) in &b.props {
        push_indent(out, indent);
        push_quoted(out, k);
        out.push(' ');
        push_quoted(out, v);
        out.push('\n');
    }
    for c in &b.children {
        write_block(c, indent, out);
    }
}

pub fn write(root: &Block) -> String {
    let mut out = String::with_capacity(4096);
    write_contents(root, 0, &mut out);
    out
}

pub fn fmt_num(out: &mut String, v: f64) {
    let v = if v == 0.0 { 0.0 } else { v };
    let _ = write!(out, "{v}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_parser_checks_nesting_before_constructing_recursive_blocks() {
        let deep = "a {".repeat(10000) + &"}".repeat(10000);
        assert!(parse_bounded(&deep, 128).is_err());
        assert!(parse_bounded("a { b { c { } } }", 2).is_err());
        assert!(parse_bounded("a { b { } }", 2).is_ok());
        let quoted = "a {\"value\" \"{{{{}}}}\" // {{{\n}";
        assert_eq!(parse_bounded(quoted, 1).unwrap().child("a").unwrap().get("value"), Some("{{{{}}}}"));
        assert!(parse_bounded("a {\"key\"", 128).is_err());
    }

    #[test]
    fn parses_vmf_like_text() {
        let src = r#"
versioninfo
{
	"editorversion" "400"
	"prefab" "0"
}
world
{
	"id" "1"
	solid
	{
		"id" "2"
		side { "id" "3" "plane" "(0 0 0) (1 0 0) (1 1 0)" }
	}
}
"#;
        let root = parse(src).unwrap();
        assert_eq!(root.children.len(), 2);
        let world = root.child("WORLD").unwrap();
        assert_eq!(world.get("id"), Some("1"));
        let solid = world.child("solid").unwrap();
        let side = solid.child("side").unwrap();
        assert_eq!(side.get_parsed::<u32>("id"), Some(3));
        assert_eq!(side.get("plane"), Some("(0 0 0) (1 0 0) (1 1 0)"));
    }

    #[test]
    fn lenient_cases() {
        let src = "// comment\n\"LightmappedGeneric\"\n{\n $basetexture concrete/floor // trailing\n \"$surfaceprop\" \"concrete\" [$WIN32]\n $dangling }\n } \n";
        let root = parse(src).unwrap();
        let m = root.child("lightmappedgeneric").unwrap();
        assert_eq!(m.get("$BASETEXTURE"), Some("concrete/floor"));
        assert_eq!(m.get("$surfaceprop"), Some("concrete"));
        assert_eq!(m.props.len(), 2);
    }

    #[test]
    fn unterminated_block_is_closed() {
        let root = parse("a { b { \"k\" \"v\" ").unwrap();
        assert_eq!(root.child("a").unwrap().child("b").unwrap().get("k"), Some("v"));
    }

    #[test]
    fn issues_are_reported() {
        let (_, issues) = parse_with_issues("a { \"k\" \"v\" }\nb { c { \"x\" \"1\" } }\n");
        assert!(issues.is_clean());
        assert!(!issues.is_truncated());

        let (root, issues) = parse_with_issues("a { b { \"plane\" \"(161 -1");
        assert_eq!(root.child("a").unwrap().child("b").unwrap().get("plane"), Some("(161 -1"));
        assert!(issues.unterminated_string);
        assert_eq!(issues.unclosed_blocks, 2);
        assert!(issues.is_truncated());

        let (_, issues) = parse_with_issues("a { \"k\"");
        assert_eq!((issues.unclosed_blocks, issues.missing_values), (1, 1));
        assert!(!issues.unterminated_string);

        let (root, issues) = parse_with_issues("} a { \"k\" } }");
        assert_eq!((issues.stray_closes, issues.missing_values, issues.unclosed_blocks), (2, 1, 0));
        assert!(root.child("a").is_some());
        assert!(!issues.is_truncated());
    }

    #[test]
    fn windows_1252_bytes_parse_losslessly() {
        let bytes = b"e { \"targetname\" \"porte_\xe9\" \"t\" \"porte_\xe8\" \"m\" \"D\xe9j\xe0 \x80\" }";
        let root = parse_bytes(bytes).unwrap();
        let e = root.child("e").unwrap();
        assert_eq!(e.get("targetname"), Some("porte_\u{e9}"));
        assert_eq!(e.get("t"), Some("porte_\u{e8}"));
        assert_eq!(e.get("m"), Some("D\u{e9}j\u{e0} \u{20ac}"));
    }

    #[test]
    fn duplicate_keys_preserved() {
        let root = parse("connections { \"OnTrigger\" \"a,Open,,0,-1\" \"OnTrigger\" \"b,Close,,1,-1\" }").unwrap();
        let c = root.child("connections").unwrap();
        assert_eq!(c.get_all("ontrigger").count(), 2);
    }

    #[test]
    fn roundtrip() {
        let src = "a\n{\n\t\"k\" \"v\"\n\tb\n\t{\n\t\t\"x\" \"1 2 3\"\n\t}\n}\n";
        let root = parse(src).unwrap();
        assert_eq!(write(&root), src);
        assert_eq!(parse(&write(&root)).unwrap(), root);
    }

    #[test]
    fn set_remove() {
        let mut b = Block::new("x");
        b.set("Key", "1");
        b.set("key", "2");
        assert_eq!(b.props.len(), 1);
        assert_eq!(b.get("KEY"), Some("2"));
        b.push_prop("key", "3");
        assert_eq!(b.remove("key"), Some("2".into()));
        assert!(b.props.is_empty());
    }

    #[test]
    fn borrowed_blocks_match_owned_blocks() {
        let sources = [
            r#"a { "k" "v" b { "x" "1 2 3" } "k" "w" }"#,
            r#"// c
"LightmappedGeneric"
{
 $basetexture a/b [$X360] "$k" "v" $dangling }
 } extra"#,
            r#"a { b { "plane" "(161 -1"#,
            r#"} a { "k" } } z { }"#,
            "\u{feff}side { id 1 dispinfo { normals { row0 \"0 0 1\" } } }",
        ];
        for src in sources {
            let owned = parse(src).unwrap();
            let borrowed = parse_ref(src);
            assert_eq!(borrowed.to_block(), owned, "{src}");
            assert_eq!(owned.as_ref(), borrowed, "{src}");
        }
        let r = parse_ref(r#"x { "Key" "1" }"#);
        assert_eq!(r.children[0].get("key"), Some("1"));
    }

    #[test]
    fn multiline_quoted_value() {
        let root = parse("\"comments\" \"line1\nline2\" \"k\" \"v\"").unwrap();
        assert_eq!(root.get("comments"), Some("line1\nline2"));
        assert_eq!(root.get("k"), Some("v"));
    }
}
