use std::ops::Range;

use crate::value::needs_quotes;
use crate::{Material, VmtError, parse, write};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Str { quoted: bool },
    Open,
    Close,
    Cond,
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    kind: Kind,
    span: (usize, usize),
    inner: (usize, usize),
}

impl Tok {
    fn start(&self) -> usize {
        self.span.0
    }

    fn end(&self) -> usize {
        self.span.1
    }

    fn text<'a>(&self, src: &'a str) -> &'a str {
        src.get(self.inner.0..self.inner.1).unwrap_or("")
    }
}

fn lex(src: &str) -> Vec<Tok> {
    let b = src.as_bytes();
    let mut pos = if src.starts_with('\u{feff}') { 3 } else { 0 };
    let mut toks = Vec::with_capacity(64);
    loop {
        loop {
            while b.get(pos).is_some_and(u8::is_ascii_whitespace) {
                pos += 1;
            }
            if b.get(pos) == Some(&b'/') && b.get(pos + 1) == Some(&b'/') {
                pos = match b.get(pos..).and_then(|rest| rest.iter().position(|&c| c == b'\n')) {
                    Some(off) => pos + off + 1,
                    None => b.len(),
                };
                continue;
            }
            break;
        }
        let Some(&c) = b.get(pos) else { break };
        let start = pos;
        let tok = match c {
            b'{' => {
                pos += 1;
                Tok { kind: Kind::Open, span: (start, pos), inner: (start, pos) }
            }
            b'}' => {
                pos += 1;
                Tok { kind: Kind::Close, span: (start, pos), inner: (start, pos) }
            }
            b'"' => {
                let inner_start = start + 1;
                let inner_end = b
                    .get(inner_start..)
                    .and_then(|rest| rest.iter().position(|&c| c == b'"'))
                    .map_or(b.len(), |off| inner_start + off);
                pos = (inner_end + 1).min(b.len());
                Tok {
                    kind: Kind::Str { quoted: true },
                    span: (start, pos),
                    inner: (inner_start.min(inner_end), inner_end),
                }
            }
            b'[' => {
                pos = b
                    .get(start..)
                    .and_then(|rest| rest.iter().position(|&c| c == b']'))
                    .map_or(b.len(), |off| start + off + 1);
                Tok { kind: Kind::Cond, span: (start, pos), inner: (start, pos) }
            }
            _ => {
                while let Some(&c) = b.get(pos) {
                    if c.is_ascii_whitespace() || c == b'{' || c == b'}' || c == b'"' {
                        break;
                    }
                    if c == b'/' && b.get(pos + 1) == Some(&b'/') {
                        break;
                    }
                    pos += 1;
                }
                Tok { kind: Kind::Str { quoted: false }, span: (start, pos), inner: (start, pos) }
            }
        };
        toks.push(tok);
    }
    toks
}

#[derive(Clone, Copy, Debug)]
struct SpanPair {
    key: Tok,
    value: Tok,
    end: usize,
}

#[derive(Debug)]
struct SpanBlock {
    name: Option<Tok>,
    open: Tok,
    pairs: Vec<SpanPair>,
    children: Vec<SpanBlock>,
    end: usize,
}

impl SpanBlock {
    fn new(name: Option<Tok>, open: Tok, end: usize) -> Self {
        Self { name, open, pairs: Vec::new(), children: Vec::new(), end }
    }

    fn name<'a>(&self, src: &'a str) -> &'a str {
        self.name.map_or("", |t| t.text(src))
    }

    fn content_end(&self) -> usize {
        self.pairs.iter().map(|p| p.end).chain(self.children.iter().map(|c| c.end)).max().unwrap_or(self.open.end())
    }
}

fn build_tree(src: &str) -> SpanBlock {
    let dummy = Tok { kind: Kind::Open, span: (0, 0), inner: (0, 0) };
    let mut stack = vec![SpanBlock::new(None, dummy, src.len())];
    let mut pending: Option<Tok> = None;
    let mut after_value = false;
    for tok in lex(src) {
        match tok.kind {
            Kind::Cond => {
                if after_value
                    && pending.is_none()
                    && let Some(pair) = stack.last_mut().and_then(|b| b.pairs.last_mut())
                {
                    pair.end = tok.end();
                }
            }
            Kind::Open => {
                after_value = false;
                stack.push(SpanBlock::new(pending.take(), tok, src.len()));
            }
            Kind::Close => {
                after_value = false;
                pending = None;
                if stack.len() > 1
                    && let Some(mut done) = stack.pop()
                    && let Some(parent) = stack.last_mut()
                {
                    done.end = tok.end();
                    parent.children.push(done);
                }
            }
            Kind::Str { .. } => match pending.take() {
                None => {
                    pending = Some(tok);
                    after_value = false;
                }
                Some(key) => {
                    if let Some(top) = stack.last_mut() {
                        top.pairs.push(SpanPair { key, value: tok, end: tok.end() });
                    }
                    after_value = true;
                }
            },
        }
    }
    while stack.len() > 1 {
        if let Some(done) = stack.pop()
            && let Some(parent) = stack.last_mut()
        {
            parent.children.push(done);
        }
    }
    stack.pop().unwrap_or_else(|| SpanBlock::new(None, dummy, src.len()))
}

#[derive(Debug)]
struct Edit {
    range: Range<usize>,
    text: String,
}

fn apply_edits(src: &str, mut edits: Vec<Edit>) -> Option<String> {
    edits.sort_by_key(|e| (e.range.start, e.range.end));
    let mut out = String::with_capacity(src.len() + 64);
    let mut cursor = 0usize;
    for e in &edits {
        if e.range.start < cursor || e.range.end < e.range.start {
            return None;
        }
        out.push_str(src.get(cursor..e.range.start)?);
        out.push_str(&e.text);
        cursor = e.range.end;
    }
    out.push_str(src.get(cursor..)?);
    Some(out)
}

fn line_start(src: &str, pos: usize) -> usize {
    src.get(..pos).and_then(|s| s.rfind('\n')).map_or(0, |i| i + 1)
}

fn line_end(src: &str, pos: usize) -> Option<usize> {
    src.get(pos..).and_then(|s| s.find('\n')).map(|i| pos + i)
}

fn is_blank_or_comment(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.starts_with("//")
}

fn removal(src: &str, pair: &SpanPair) -> Edit {
    let start = pair.key.start();
    let end = pair.end;
    let ls = line_start(src, start);
    let before = src.get(ls..start).unwrap_or("x");
    let le = line_end(src, end);
    let after = src.get(end..le.unwrap_or(src.len())).unwrap_or("x");
    if before.trim().is_empty() && is_blank_or_comment(after) {
        let stop = le.map_or(src.len(), |nl| nl + 1);
        return Edit { range: ls..stop, text: String::new() };
    }
    let bytes = src.as_bytes();
    let mut stop = end;
    while matches!(bytes.get(stop), Some(b' ' | b'\t')) {
        stop += 1;
    }
    Edit { range: start..stop, text: String::new() }
}

fn format_value(value: &str, quoted: bool) -> String {
    if quoted || needs_quotes(value) { format!("\"{value}\"") } else { value.to_owned() }
}

fn indent_for(src: &str, block: &SpanBlock) -> String {
    for pair in &block.pairs {
        let ls = line_start(src, pair.key.start());
        if let Some(ws) = src.get(ls..pair.key.start())
            && ws.chars().all(|c| c == ' ' || c == '\t')
        {
            return ws.to_owned();
        }
    }
    let ls = line_start(src, block.open.start());
    let base: String =
        src.get(ls..block.open.start()).unwrap_or("").chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    format!("{base}\t")
}

fn eol_of(src: &str) -> &'static str {
    if src.contains("\r\n") { "\r\n" } else { "\n" }
}

fn insert_lines(src: &str, anchor: usize, lines: &[String], inline: &str) -> Edit {
    let eol = eol_of(src);
    let le = line_end(src, anchor);
    let rest = src.get(anchor..le.unwrap_or(src.len())).unwrap_or("x");
    if !is_blank_or_comment(rest) {
        return Edit { range: anchor..anchor, text: format!(" {inline}") };
    }
    let mut text = String::new();
    match le {
        Some(nl) => {
            for line in lines {
                text.push_str(line);
                text.push_str(eol);
            }
            Edit { range: nl + 1..nl + 1, text }
        }
        None => {
            for line in lines {
                text.push_str(eol);
                text.push_str(line);
            }
            Edit { range: src.len()..src.len(), text }
        }
    }
}

fn insertion(src: &str, block: &SpanBlock, key: &str, value: &str) -> Edit {
    let anchor = block.pairs.iter().map(|p| p.end).max().unwrap_or(block.open.end());
    let pair = format!("\"{key}\" \"{value}\"");
    let indent = indent_for(src, block);
    insert_lines(src, anchor, &[format!("{indent}{pair}")], &pair)
}

fn block_insertion(src: &str, block: &SpanBlock, name: &str, key: &str, value: &str) -> Edit {
    let indent = indent_for(src, block);
    let pair = format!("\"{key}\" \"{value}\"");
    let lines = [format!("{indent}{name}"), format!("{indent}{{"), format!("{indent}\t{pair}"), format!("{indent}}}")];
    insert_lines(src, block.content_end(), &lines, &format!("{name} {{ {pair} }}"))
}

fn set_edits(src: &str, block: &SpanBlock, key: &str, value: &str) -> Vec<Edit> {
    let mut matches = block.pairs.iter().filter(|p| p.key.text(src).eq_ignore_ascii_case(key));
    let Some(first) = matches.next() else {
        return vec![insertion(src, block, key, value)];
    };
    let quoted = matches!(first.value.kind, Kind::Str { quoted: true });
    let mut edits = vec![Edit { range: first.value.start()..first.value.end(), text: format_value(value, quoted) }];
    edits.extend(matches.map(|p| removal(src, p)));
    edits
}

fn validate(key: &str, value: Option<&str>) -> Result<(), VmtError> {
    if key.is_empty() {
        return Err(VmtError::InvalidParam("empty key".to_owned()));
    }
    if key.contains('"') {
        return Err(VmtError::InvalidParam(format!("key contains a double quote: {key}")));
    }
    if let Some(v) = value
        && v.contains('"')
    {
        return Err(VmtError::InvalidParam(format!("value of {key} contains a double quote")));
    }
    Ok(())
}

fn patch_target(mat: &Material, key: &str) -> bool {
    mat.is_patch() && !key.eq_ignore_ascii_case("include")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditOutcome {
    pub text: String,
    pub fallback: bool,
}

fn finish(candidate: Option<String>, expected: &Material) -> EditOutcome {
    if let Some(text) = candidate
        && parse(&text).ok().as_ref() == Some(expected)
    {
        return EditOutcome { text, fallback: false };
    }
    EditOutcome { text: write(expected), fallback: true }
}

pub fn set_param_in_text_detailed(vmt_text: &str, key: &str, value: &str) -> Result<EditOutcome, VmtError> {
    validate(key, Some(value))?;
    let original = parse(vmt_text)?;
    let mut expected = original.clone();
    expected.set_patch_aware(key, value);

    let tree = build_tree(vmt_text);
    let Some(shader) = tree.children.first() else {
        return Ok(finish(None, &expected));
    };
    let edits = if patch_target(&original, key) {
        let replace = shader.children.iter().find(|c| c.name(vmt_text).eq_ignore_ascii_case("replace"));
        let has_key = |b: &&SpanBlock| b.pairs.iter().any(|p| p.key.text(vmt_text).eq_ignore_ascii_case(key));
        let insert = || shader.children.iter().find(|c| c.name(vmt_text).eq_ignore_ascii_case("insert"));
        match replace.filter(has_key).or_else(insert) {
            Some(block) => set_edits(vmt_text, block, key, value),
            None => vec![block_insertion(vmt_text, shader, "insert", key, value)],
        }
    } else {
        set_edits(vmt_text, shader, key, value)
    };
    Ok(finish(apply_edits(vmt_text, edits), &expected))
}

pub fn remove_param_in_text_detailed(vmt_text: &str, key: &str) -> Result<EditOutcome, VmtError> {
    validate(key, None)?;
    let original = parse(vmt_text)?;
    let mut expected = original.clone();
    if !expected.remove_patch_aware(key) {
        return Ok(EditOutcome { text: vmt_text.to_owned(), fallback: false });
    }

    let tree = build_tree(vmt_text);
    let Some(shader) = tree.children.first() else {
        return Ok(finish(None, &expected));
    };
    let containers: Vec<&SpanBlock> = if patch_target(&original, key) {
        shader
            .children
            .iter()
            .filter(|c| {
                let n = c.name(vmt_text);
                n.eq_ignore_ascii_case("insert") || n.eq_ignore_ascii_case("replace")
            })
            .collect()
    } else {
        vec![shader]
    };
    let edits: Vec<Edit> = containers
        .iter()
        .flat_map(|block| block.pairs.iter())
        .filter(|p| p.key.text(vmt_text).eq_ignore_ascii_case(key))
        .map(|p| removal(vmt_text, p))
        .collect();
    Ok(finish(apply_edits(vmt_text, edits), &expected))
}

pub fn set_param_in_text(vmt_text: &str, key: &str, value: &str) -> Result<String, VmtError> {
    set_param_in_text_detailed(vmt_text, key, value).map(|o| o.text)
}

pub fn set_param_everywhere_in_text(vmt_text: &str, key: &str, value: &str) -> Result<String, VmtError> {
    validate(key, Some(value))?;
    let original = parse(vmt_text)?;
    let seeded = original.is_patch().then(|| set_param_in_text(vmt_text, key, value)).transpose()?;
    let vmt_text = seeded.as_deref().unwrap_or(vmt_text);
    let original = parse(vmt_text)?;
    fn set_nested(block: &mut hp_kv::Block, key: &str, value: &str) -> bool {
        let mut found = false;
        if block.get(key).is_some() {
            crate::material::set_unique(&mut block.props, key, value);
            found = true;
        }
        for child in &mut block.children {
            found |= set_nested(child, key, value);
        }
        found
    }
    fn nested_edits(src: &str, block: &SpanBlock, key: &str, value: &str, out: &mut Vec<Edit>) {
        for child in &block.children {
            if child.pairs.iter().any(|p| p.key.text(src).eq_ignore_ascii_case(key)) {
                out.extend(set_edits(src, child, key, value));
            }
            nested_edits(src, child, key, value, out);
        }
    }
    let mut expected = original.clone();
    let mut nested = false;
    for block in expected.other_blocks.iter_mut().chain(expected.proxies.as_mut()) {
        nested |= set_nested(block, key, value);
    }
    let top = !original.is_patch() && (original.has(key) || !nested);
    if top {
        expected.set(key, value);
    }
    let tree = build_tree(vmt_text);
    let Some(shader) = tree.children.first() else {
        return Ok(finish(None, &expected).text);
    };
    let mut edits = if top { set_edits(vmt_text, shader, key, value) } else { Vec::new() };
    nested_edits(vmt_text, shader, key, value, &mut edits);
    Ok(finish(apply_edits(vmt_text, edits), &expected).text)
}

/// Set a material parameter and its overrides, including sections inherited by a
/// Patch. `resolved` must retain the DX/HDR sections (resolve includes, not fallbacks).
/// Only the edited VMT is changed; included source materials remain untouched.
pub fn set_param_with_fallbacks_in_text(
    vmt_text: &str,
    key: &str,
    value: &str,
    resolved: &Material,
) -> Result<String, VmtError> {
    let mut text = set_param_everywhere_in_text(vmt_text, key, value)?;
    if !parse(vmt_text)?.is_patch() {
        return Ok(text);
    }
    fn paths<'a>(block: &'a hp_kv::Block, key: &str, mut path: Vec<&'a str>, out: &mut Vec<Vec<&'a str>>) {
        if block.name.eq_ignore_ascii_case("proxies") {
            return;
        }
        path.push(&block.name);
        if block.get(key).is_some() {
            out.push(path.clone());
        }
        for child in &block.children {
            paths(child, key, path.clone(), out);
        }
    }
    let mut overrides = Vec::new();
    for block in &resolved.other_blocks {
        paths(block, key, Vec::new(), &mut overrides);
    }
    for path in overrides {
        let path: Vec<_> = std::iter::once("insert").chain(path).collect();
        text = set_param_at_path(&text, &path, key, value)?;
    }
    Ok(text)
}

fn set_param_at_path(vmt_text: &str, path: &[&str], key: &str, value: &str) -> Result<String, VmtError> {
    let mut expected = parse(vmt_text)?.to_block();
    let mut target = &mut expected;
    for name in path {
        let index = match target.children.iter().position(|child| child.name.eq_ignore_ascii_case(name)) {
            Some(index) => index,
            None => {
                target.children.push(hp_kv::Block::new(*name));
                target.children.len() - 1
            }
        };
        target = &mut target.children[index];
    }
    crate::material::set_unique(&mut target.props, key, value);
    let expected = Material::from_block(expected);
    let tree = build_tree(vmt_text);
    let Some(mut block) = tree.children.first() else {
        return Ok(finish(None, &expected).text);
    };
    let mut missing = None;
    for (index, name) in path.iter().enumerate() {
        match block.children.iter().find(|child| child.name(vmt_text).eq_ignore_ascii_case(name)) {
            Some(child) => block = child,
            None => {
                missing = Some(index);
                break;
            }
        }
    }
    let edits = if let Some(index) = missing {
        let mut child = hp_kv::Block::new(*path.last().unwrap());
        child.props.push((key.to_owned(), value.to_owned()));
        for name in path[index..path.len() - 1].iter().rev() {
            let mut parent = hp_kv::Block::new(*name);
            parent.children.push(child);
            child = parent;
        }
        let text = write(&Material::from_block(child));
        let indent = indent_for(vmt_text, block);
        let lines: Vec<_> = text.trim_end().lines().map(|line| format!("{indent}{line}")).collect();
        vec![insert_lines(vmt_text, block.content_end(), &lines, text.trim_end())]
    } else {
        set_edits(vmt_text, block, key, value)
    };
    Ok(finish(apply_edits(vmt_text, edits), &expected).text)
}

pub fn remove_param_in_text(vmt_text: &str, key: &str) -> Result<String, VmtError> {
    remove_param_in_text_detailed(vmt_text, key).map(|o| o.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_everywhere_reaches_fallback_sections() {
        let text = "\"LightmappedGeneric\"
{
	\"$basetexture\" \"a/b\"
	\"LightmappedGeneric_DX9\"
	{
		\"$bumpmap\" \"a/b_normal\" // keep
	}
}
";
        let out = set_param_everywhere_in_text(text, "$bumpmap", "v/baked").unwrap();
        assert!(out.contains("\"$bumpmap\" \"v/baked\" // keep"), "{out}");
        let m = parse(&out).unwrap();
        assert!(!m.has("$bumpmap"), "not added on top of the section");
        assert_eq!(m.block("LightmappedGeneric_DX9").unwrap().get("$bumpmap"), Some("v/baked"));
        let out = set_param_everywhere_in_text(text, "%hpbumpsource", "a/b_normal").unwrap();
        assert_eq!(parse(&out).unwrap().get("%hpbumpsource"), Some("a/b_normal"));
    }

    #[test]
    fn patch_edits_update_inherited_and_explicit_fallback_overrides_without_touching_the_include() {
        let base = r#"VertexLitGeneric {
            "$detailscale" "4"
            VertexLitGeneric_DX9 { "$detailscale" "8" "$detail" "original/detail" }
            VertexLitGeneric_DX9_HDR { "$detailscale" "12" }
        }"#;
        for text in [
            "// keep\nPatch { include base }",
            "// keep\nPatch { include base insert { VertexLitGeneric_DX9 { \"$detailscale\" \"20\" } } replace { \"$detailscale\" \"24\" } }",
            "// keep\nPatch\r\n{\r\n\tinclude base\r\n\treplace { VertexLitGeneric_DX9_HDR { \"$detailscale\" \"28\" } }\r\n}\r\n",
        ] {
            let load = |_: &str| Some(base.to_owned());
            let resolved = crate::resolve_patch(&parse(text).unwrap(), &load).unwrap();
            let out = set_param_with_fallbacks_in_text(text, "$detailscale", "16", &resolved).unwrap();
            assert!(out.starts_with("// keep"), "{out}");
            assert_eq!(parse(&out).unwrap().get("include"), Some("base"));
            let result = crate::resolve_patch(&parse(&out).unwrap(), &load).unwrap();
            for hdr in [false, true] {
                let result = result.resolve_fallbacks(95, hdr);
                assert_eq!(result.get("$detailscale"), Some("16"), "HDR{hdr}: {out}");
                assert_eq!(result.get("$detail"), Some("original/detail"), "unrelated branch data stays intact");
            }
        }
    }

    const SRC: &str = "// Header comment\r\n\"LightmappedGeneric\"\r\n{\r\n\t\"$basetexture\" \"concrete/floor01\" // the base\r\n\t$surfaceprop concrete\r\n\t\"$envmap\" \"env_cubemap\" [!$X360]\r\n\r\n\t\"Proxies\"\r\n\t{\r\n\t\t\"Sine\" { \"resultvar\" \"$alpha\" }\r\n\t}\r\n}\r\n";

    fn set_ok(src: &str, k: &str, v: &str) -> String {
        let out = set_param_in_text_detailed(src, k, v).unwrap();
        assert!(!out.fallback, "unexpected fallback for {k}:\n{src}\n=>\n{}", out.text);
        out.text
    }

    fn remove_ok(src: &str, k: &str) -> String {
        let out = remove_param_in_text_detailed(src, k).unwrap();
        assert!(!out.fallback, "unexpected fallback for {k}:\n{src}\n=>\n{}", out.text);
        out.text
    }

    #[test]
    fn replace_existing_value() {
        let out = set_ok(SRC, "$BASETEXTURE", "brick/wall02");
        assert_eq!(out, SRC.replace("\"concrete/floor01\"", "\"brick/wall02\""));
        let out = set_ok(SRC, "$surfaceprop", "metal");
        assert_eq!(out, SRC.replace("$surfaceprop concrete", "$surfaceprop metal"));
        let out = set_ok(SRC, "$surfaceprop", "a b");
        assert_eq!(out, SRC.replace("$surfaceprop concrete", "$surfaceprop \"a b\""));
        let out = set_ok(SRC, "$envmap", "maps/x/c1");
        assert!(out.contains("\"$envmap\" \"maps/x/c1\" [!$X360]"));
    }

    #[test]
    fn insert_new_param() {
        let out = set_ok(SRC, "$detail", "detail/noise");
        let expected = SRC.replace("[!$X360]\r\n", "[!$X360]\r\n\t\"$detail\" \"detail/noise\"\r\n");
        assert_eq!(out, expected);
        let m = parse(&out).unwrap();
        assert_eq!(m.params.last().unwrap(), &("$detail".to_owned(), "detail/noise".to_owned()));
    }

    #[test]
    fn insert_after_comment_line_and_empty_blocks() {
        let src = "X\n{\n    $a 1 // note\n}\n";
        assert_eq!(set_ok(src, "$b", "2"), "X\n{\n    $a 1 // note\n    \"$b\" \"2\"\n}\n");
        let src = "X\n{\n}\n";
        assert_eq!(set_ok(src, "$b", "2"), "X\n{\n\t\"$b\" \"2\"\n}\n");
        let src = "  X\n  {\n  }";
        assert_eq!(set_ok(src, "$b", "2"), "  X\n  {\n  \t\"$b\" \"2\"\n  }");
        let src = "X { $a 1 }";
        assert_eq!(set_ok(src, "$b", "2"), "X { $a 1 \"$b\" \"2\" }");
        let src = "X {";
        assert_eq!(set_ok(src, "$b", "2"), "X {\n\t\"$b\" \"2\"");
        let src = "X\n{\n\t$a 1";
        assert_eq!(set_ok(src, "$b", "2"), "X\n{\n\t$a 1\n\t\"$b\" \"2\"");
    }

    #[test]
    fn duplicates_are_collapsed() {
        let src = "X\n{\n\t$a 1\n\t$b 2\n\t$A 3 // dup\n\t$c 4\n}\n";
        assert_eq!(set_ok(src, "$a", "9"), "X\n{\n\t$a 9\n\t$b 2\n\t$c 4\n}\n");
        let src = "X { $a 1 $a 2 $b 3 }";
        assert_eq!(set_ok(src, "$a", "9"), "X { $a 9 $b 3 }");
    }

    #[test]
    fn remove_params() {
        let out = remove_ok(SRC, "$basetexture");
        assert_eq!(out, SRC.replace("\t\"$basetexture\" \"concrete/floor01\" // the base\r\n", ""));
        let out = remove_ok(SRC, "$ENVMAP");
        assert_eq!(out, SRC.replace("\t\"$envmap\" \"env_cubemap\" [!$X360]\r\n", ""));
        assert_eq!(remove_ok(SRC, "$missing"), SRC);
        assert_eq!(remove_ok("X { $a 1 $b 2 }", "$a"), "X { $b 2 }");
        assert_eq!(remove_ok("X { $a 1 $b 2 }", "$b"), "X { $a 1 }");
        assert_eq!(remove_ok("X\n{\n$a 1\n$a 2", "$a"), "X\n{\n");
        let src = "X { $a 1 Proxies { Sine { $a 2 } } }";
        assert_eq!(remove_ok(src, "$a"), "X { Proxies { Sine { $a 2 } } }");
    }

    #[test]
    fn nested_keys_are_not_replaced() {
        let src = "X\n{\n\tProxies { Sine { resultvar $color } }\n}\n";
        let out = set_ok(src, "resultvar", "1");
        assert!(out.contains("Sine { resultvar $color }"));
        assert_eq!(parse(&out).unwrap().get("resultvar"), Some("1"));
    }

    #[test]
    fn patch_materials() {
        let src = "patch\n{\n\tinclude \"materials/a/b.vmt\"\n\treplace\n\t{\n\t\t\"$envmap\" \"maps/m/c0\"\n\t}\n\tinsert\n\t{\n\t\t\"$detail\" \"x\"\n\t}\n}\n";
        let out = set_ok(src, "$envmap", "maps/m/c1");
        assert!(out.contains("\"$envmap\" \"maps/m/c1\""));
        let out = set_ok(src, "$surfaceprop", "metal");
        assert!(out.contains("\t\t\"$detail\" \"x\"\n\t\t\"$surfaceprop\" \"metal\"\n"));
        let out = set_ok(src, "include", "materials/c.vmt");
        assert!(out.contains("include \"materials/c.vmt\""));
        let out = remove_ok(src, "$envmap");
        assert!(!out.contains("$envmap"));
        assert!(out.contains("replace\n\t{\n\t}"));

        let src = "patch\n{\n\tinclude \"materials/a.vmt\"\n\treplace\n\t{\n\t\t$envmap x\n\t}\n}\n";
        let out = set_ok(src, "$basetexture", "q");
        assert_eq!(
            out,
            "patch\n{\n\tinclude \"materials/a.vmt\"\n\treplace\n\t{\n\t\t$envmap x\n\t}\n\tinsert\n\t{\n\t\t\"$basetexture\" \"q\"\n\t}\n}\n"
        );
        let src = "patch { include \"materials/a.vmt\" }";
        let out = set_ok(src, "$basetexture", "q");
        assert_eq!(out, "patch { include \"materials/a.vmt\" insert { \"$basetexture\" \"q\" } }");
        let m = parse(&out).unwrap();
        assert_eq!(m.block("insert").unwrap().get("$basetexture"), Some("q"));

        let src = "patch { include \"materials/a.vmt\" replace { $envmap x";
        let out = set_param_in_text_detailed(src, "$basetexture", "q").unwrap();
        assert!(out.fallback);
        let m = parse(&out.text).unwrap();
        assert_eq!(m.block("insert").unwrap().get("$basetexture"), Some("q"));
        assert_eq!(m.block("replace").unwrap().get("$envmap"), Some("x"));
    }

    #[test]
    fn fallback_when_text_is_ambiguous() {
        let src = "X { $a [$WIN32] 1 }";
        assert_eq!(set_ok(src, "$a", "2"), "X { $a [$WIN32] 2 }");
        let src = "junk_key junk_value\nX { $a 1 }\nY { $a 5 }\n";
        assert_eq!(set_ok(src, "$a", "2"), "junk_key junk_value\nX { $a 2 }\nY { $a 5 }\n");
    }

    #[test]
    fn errors() {
        assert!(matches!(set_param_in_text(SRC, "", "x"), Err(VmtError::InvalidParam(_))));
        assert!(matches!(set_param_in_text(SRC, "$a\"", "x"), Err(VmtError::InvalidParam(_))));
        assert!(matches!(set_param_in_text(SRC, "$a", "x\"y"), Err(VmtError::InvalidParam(_))));
        assert!(matches!(remove_param_in_text(SRC, ""), Err(VmtError::InvalidParam(_))));
        assert!(matches!(set_param_in_text("// nothing", "$a", "1"), Err(VmtError::NoShader)));
        assert!(matches!(remove_param_in_text("", "$a"), Err(VmtError::NoShader)));
    }

    #[test]
    fn bom_and_values_needing_quotes() {
        let src = "\u{feff}X\n{\n\t$a 1\n}\n";
        let out = set_ok(src, "$a", "[1 1 1]");
        assert_eq!(out, "\u{feff}X\n{\n\t$a \"[1 1 1]\"\n}\n");
        let out = set_ok(src, "$a", "");
        assert_eq!(parse(&out).unwrap().get("$a"), Some(""));
        let out = set_ok(src, "$new key", "{255 0 0}");
        assert_eq!(parse(&out).unwrap().get("$new key"), Some("{255 0 0}"));
    }

    #[test]
    fn unterminated_quote_value() {
        let src = "X { \"$a\" \"unterminated";
        let out = set_param_in_text(src, "$a", "b").unwrap();
        assert_eq!(parse(&out).unwrap().get("$a"), Some("b"));
    }

    #[test]
    fn public_wrappers() {
        let out = set_param_in_text(SRC, "$detail", "d").unwrap();
        assert_eq!(parse(&out).unwrap().get("$detail"), Some("d"));
        let out = remove_param_in_text(&out, "$detail").unwrap();
        assert_eq!(out, SRC);
    }

    #[test]
    fn lexer_spans() {
        let src = "\"a b\" c{}[$X] // c\n\"open";
        let toks = lex(src);
        assert_eq!(toks.len(), 6);
        assert_eq!(toks[0].text(src), "a b");
        assert_eq!(toks[1].text(src), "c");
        assert_eq!(toks[2].kind, Kind::Open);
        assert_eq!(toks[3].kind, Kind::Close);
        assert_eq!(toks[4].kind, Kind::Cond);
        assert_eq!(toks[5].text(src), "open");
        assert_eq!(toks[5].end(), src.len());
    }
}
