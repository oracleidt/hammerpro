#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pass {
    Both,
    Ldr,
    Hdr,
}

impl Pass {
    fn prefix(self) -> &'static str {
        match self {
            Pass::Both => "",
            Pass::Ldr => "ldr:",
            Pass::Hdr => "hdr:",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pass::Both => "both",
            Pass::Ldr => "ldr",
            Pass::Hdr => "hdr",
        }
    }

    pub fn parse(text: &str) -> Option<Pass> {
        match text.trim().to_ascii_lowercase().as_str() {
            "" | "both" => Some(Pass::Both),
            "ldr" => Some(Pass::Ldr),
            "hdr" => Some(Pass::Hdr),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TexLightLine {
    pub pass: Pass,
    pub material: String,
    pub color: [f32; 3],
    pub brightness: f32,
    pub hdr: Option<([f32; 3], f32)>,
}

impl TexLightLine {
    fn text(&self) -> String {
        let n = |v: f32| {
            let r = (v * 1000.0).round() / 1000.0;
            if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
        };
        let mut out = format!(
            "{}{} {} {} {} {}",
            self.pass.prefix(),
            self.material,
            n(self.color[0]),
            n(self.color[1]),
            n(self.color[2]),
            n(self.brightness)
        );
        if let Some((c, b)) = self.hdr {
            out.push_str(&format!(" {} {} {} {}", n(c[0]), n(c[1]), n(c[2]), n(b)));
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Line {
    Other(String),
    TexLight(TexLightLine),
    NoShadow(String),
    ForceTextureShadow(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadDoc {
    lines: Vec<Line>,
}

fn strip_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| s.get(prefix.len()..).unwrap_or(""))
}

fn same(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().replace('\\', "/").to_ascii_lowercase();
    norm(a) == norm(b)
}

fn parse_line(raw: &str) -> Line {
    let body = raw.split("//").next().unwrap_or("").trim();
    if body.is_empty() {
        return Line::Other(raw.to_owned());
    }
    let (pass, rest) = if let Some(r) = strip_ci(body, "hdr:") {
        (Pass::Hdr, r.trim_start())
    } else if let Some(r) = strip_ci(body, "ldr:") {
        (Pass::Ldr, r.trim_start())
    } else {
        (Pass::Both, body)
    };
    let mut words = rest.split_whitespace();
    let Some(first) = words.next() else { return Line::Other(raw.to_owned()) };
    let lower = first.to_ascii_lowercase();
    if pass == Pass::Both && (lower == "noshadow" || lower == "forcetextureshadow") {
        let Some(name) = words.next() else { return Line::Other(raw.to_owned()) };
        return if lower == "noshadow" {
            Line::NoShadow(name.to_owned())
        } else {
            Line::ForceTextureShadow(name.to_owned())
        };
    }
    let values: Vec<f32> = words.map_while(|w| w.parse::<f32>().ok()).collect();
    let (color, brightness, hdr) = match values.as_slice() {
        [v] => ([*v; 3], 255.0, None),
        [r, g, b] => ([*r, *g, *b], 255.0, None),
        [r, g, b, s] => ([*r, *g, *b], *s, None),
        [r, g, b, s, hr, hg, hb, hs, ..] => ([*r, *g, *b], *s, Some(([*hr, *hg, *hb], *hs))),
        _ => return Line::Other(raw.to_owned()),
    };
    if raw.contains("//") || !matches!(values.len(), 1 | 3 | 4 | 8) {
        return Line::Other(raw.to_owned());
    }
    Line::TexLight(TexLightLine { pass, material: first.to_owned(), color, brightness, hdr })
}

impl RadDoc {
    pub fn parse(text: &str) -> RadDoc {
        RadDoc { lines: text.lines().map(parse_line).collect() }
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Other(t) => out.push_str(t),
                Line::TexLight(t) => out.push_str(&t.text()),
                Line::NoShadow(n) => out.push_str(&format!("noshadow {n}")),
                Line::ForceTextureShadow(m) => out.push_str(&format!("forcetextureshadow {m}")),
            }
            out.push_str("\r\n");
        }
        out
    }

    pub fn texlights(&self) -> Vec<TexLightLine> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::TexLight(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn no_shadow(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::NoShadow(n) => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn force_texture_shadow(&self) -> Vec<String> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::ForceTextureShadow(m) => Some(m.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn set_texlight(&mut self, light: TexLightLine) {
        let slot = self
            .lines
            .iter_mut()
            .find(|l| matches!(l, Line::TexLight(t) if t.pass == light.pass && same(&t.material, &light.material)));
        match slot {
            Some(l) => *l = Line::TexLight(light),
            None => self.lines.push(Line::TexLight(light)),
        }
    }

    pub fn remove_texlight(&mut self, material: &str, pass: Option<Pass>) -> usize {
        let before = self.lines.len();
        self.lines.retain(
            |l| !matches!(l, Line::TexLight(t) if same(&t.material, material) && pass.is_none_or(|p| p == t.pass)),
        );
        before - self.lines.len()
    }

    pub fn add_no_shadow(&mut self, material: &str) -> bool {
        let m = material.trim();
        if m.is_empty() || self.no_shadow().iter().any(|n| same(n, m)) {
            return false;
        }
        self.lines.push(Line::NoShadow(m.to_owned()));
        true
    }

    pub fn remove_no_shadow(&mut self, material: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|l| !matches!(l, Line::NoShadow(n) if same(n, material)));
        before != self.lines.len()
    }

    pub fn add_force_texture_shadow(&mut self, model: &str) -> bool {
        let m = model.trim();
        if m.is_empty() || self.force_texture_shadow().iter().any(|n| same(n, m)) {
            return false;
        }
        self.lines.push(Line::ForceTextureShadow(m.to_owned()));
        true
    }

    pub fn remove_force_texture_shadow(&mut self, model: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|l| !matches!(l, Line::ForceTextureShadow(n) if same(n, model)));
        before != self.lines.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_the_other_lines() {
        let text = "// my lights\nlights/white001 255 255 255 200\nhdr:lights/white001 255 255 255 800\n\
            lights/red 255 0 0 50 255 0 0 400\nnoshadow foliage/leaves\nforcetextureshadow props/fence.mdl\nweird line here\n";
        let mut doc = RadDoc::parse(text);
        assert_eq!(doc.texlights().len(), 3);
        assert_eq!(doc.texlights()[2].hdr, Some(([255.0, 0.0, 0.0], 400.0)));
        assert_eq!(doc.texlights()[1].pass, Pass::Hdr);
        assert_eq!(doc.no_shadow(), ["foliage/leaves"]);
        assert_eq!(doc.force_texture_shadow(), ["props/fence.mdl"]);
        doc.set_texlight(TexLightLine {
            pass: Pass::Both,
            material: "LIGHTS/WHITE001".into(),
            color: [255.0, 200.0, 150.0],
            brightness: 300.0,
            hdr: None,
        });
        doc.set_texlight(TexLightLine {
            pass: Pass::Both,
            material: "lights/panel".into(),
            color: [10.0, 20.0, 30.5],
            brightness: 100.0,
            hdr: Some(([1.0, 2.0, 3.0], 4.0)),
        });
        assert!(doc.add_force_texture_shadow("models/props/tree.mdl"));
        assert!(!doc.add_force_texture_shadow("PROPS/FENCE.MDL"));
        assert!(doc.remove_no_shadow("foliage/leaves"));
        assert_eq!(doc.remove_texlight("lights/red", None), 1);
        let out = doc.text();
        assert!(
            out.starts_with(
                "// my lights\r\nLIGHTS/WHITE001 255 200 150 300\r\nhdr:lights/white001 255 255 255 800\r\n"
            ),
            "{out}"
        );
        assert!(out.contains("weird line here\r\n"));
        assert!(out.contains("lights/panel 10 20 30.5 100 1 2 3 4\r\n"));
        assert!(out.contains("forcetextureshadow models/props/tree.mdl"));
        assert!(!out.contains("noshadow") && !out.contains("lights/red"));
        let again = RadDoc::parse(&out);
        assert_eq!(again.texlights().len(), 3);
        assert_eq!(again.text(), out);
    }
}
