//! A small tolerant XML reader for MPD manifests (elements, attributes, text; namespace
//! prefixes dropped).

// ---------------------------------------------------------------------------------------------
// XML

#[derive(Debug, Default, Clone)]
pub struct Node {
    /// Local name (namespace prefix removed).
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        if let Some(end) = rest.find(';').filter(|&e| e <= 12) {
            let ent = &rest[1..end];
            let ch = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
                }
                _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(c) = ch {
                out.push(c);
                rest = &rest[end + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn find_from(src: &str, from: usize, pat: &str) -> Result<usize, String> {
    src.get(from..)
        .and_then(|s| s.find(pat))
        .map(|p| from + p)
        .ok_or_else(|| format!("unterminated markup (expected {pat})"))
}

/// Parses a start tag beginning after `<`. Returns the node, whether it closed itself, and the
/// position after `>`.
fn start_tag(src: &str, mut i: usize) -> Result<(Node, bool, usize), String> {
    let b = src.as_bytes();
    let ws = |c: u8| c == b' ' || c == b'\t' || c == b'\r' || c == b'\n';
    let name_start = i;
    while i < b.len() && !ws(b[i]) && b[i] != b'>' && b[i] != b'/' {
        i += 1;
    }
    let mut node = Node { name: local(&src[name_start..i]).to_string(), ..Default::default() };
    loop {
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            return Err("unterminated tag".into());
        }
        match b[i] {
            b'>' => return Ok((node, false, i + 1)),
            b'/' => {
                let close = find_from(src, i, ">")?;
                return Ok((node, true, close + 1));
            }
            _ => {}
        }
        let an = i;
        while i < b.len() && !ws(b[i]) && b[i] != b'=' && b[i] != b'>' && b[i] != b'/' {
            i += 1;
        }
        let name = src[an..i].to_string();
        while i < b.len() && ws(b[i]) {
            i += 1;
        }
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && ws(b[i]) {
                i += 1;
            }
            let quote = *b.get(i).ok_or("unterminated attribute")?;
            if quote == b'"' || quote == b'\'' {
                let end = find_from(src, i + 1, if quote == b'"' { "\"" } else { "'" })?;
                node.attrs.push((name, decode(&src[i + 1..end])));
                i = end + 1;
            } else {
                let vs = i;
                while i < b.len() && !ws(b[i]) && b[i] != b'>' {
                    i += 1;
                }
                node.attrs.push((name, decode(&src[vs..i])));
            }
        } else if !name.is_empty() {
            node.attrs.push((name, String::new()));
        } else {
            i += 1; // stray character: skip it
        }
    }
}

/// Parses an XML document and returns its root element.
pub fn parse_xml(src: &str) -> Result<Node, String> {
    let src = src.trim_start_matches('\u{feff}');
    let mut stack = vec![Node::default()];
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        if rest.starts_with('<') {
            if rest.starts_with("<!--") {
                i = find_from(src, i + 4, "-->")? + 3;
            } else if rest.starts_with("<![CDATA[") {
                let end = find_from(src, i + 9, "]]>")?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&src[i + 9..end]);
                }
                i = end + 3;
            } else if rest.starts_with("<?") {
                i = find_from(src, i + 2, "?>")? + 2;
            } else if rest.starts_with("<!") {
                i = find_from(src, i + 2, ">")? + 1;
            } else if rest.starts_with("</") {
                i = find_from(src, i + 2, ">")? + 1;
                if stack.len() > 1 {
                    let node = stack.pop().unwrap_or_default();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                }
            } else {
                let (node, closed, next) = start_tag(src, i + 1)?;
                if closed {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    }
                } else {
                    stack.push(node);
                }
                i = next;
            }
        } else {
            let end = rest.find('<').map_or(src.len(), |p| i + p);
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&decode(&src[i..end]));
            }
            i = end;
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap_or_default();
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
        }
    }
    stack.pop().and_then(|root| root.children.into_iter().next()).ok_or_else(|| "empty document".into())
}
