//! Netscape bookmark files (what Firefox, Chrome and Edge export and import): the export, a
//! tolerant parser and the import that merges a file into the tree.

use rusqlite::{Connection, TransactionBehavior};

use super::tree::{children_ids, insert_one, Ctx, Tree};
use super::validate::{clean_keyword, clean_tags, clean_title, clean_url, Prepared};
use super::{Kind, MAX_DEPTH, MAX_NODES, ROOT_MENU, ROOT_OTHER, ROOT_TOOLBAR};
use crate::storage::now_ms;

// ---------------------------------------------------------------------------------------------
// Netscape bookmark files (what Firefox, Chrome and Edge export and import)

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn html_children(out: &mut String, tree: &Tree, parent: i64, depth: usize) {
    let pad = "    ".repeat(depth);
    for n in tree.kids(parent) {
        match n.kind {
            Kind::Separator => out.push_str(&format!("{pad}<HR>\n")),
            Kind::Folder => {
                out.push_str(&format!(
                    "{pad}<DT><H3 ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\">{}</H3>\n{pad}<DL><p>\n",
                    n.created / 1000,
                    n.modified / 1000,
                    html_escape(&n.title)
                ));
                html_children(out, tree, n.id, depth + 1);
                out.push_str(&format!("{pad}</DL><p>\n"));
            }
            Kind::Bookmark => {
                let mut extra = String::new();
                if !n.keyword.is_empty() {
                    extra.push_str(&format!(" SHORTCUTURL=\"{}\"", html_escape(&n.keyword)));
                }
                if !n.tags.is_empty() {
                    extra.push_str(&format!(" TAGS=\"{}\"", html_escape(&n.tags.join(","))));
                }
                out.push_str(&format!(
                    "{pad}<DT><A HREF=\"{}\" ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\"{extra}>{}</A>\n",
                    html_escape(&n.url),
                    n.created / 1000,
                    n.modified / 1000,
                    html_escape(&n.title)
                ));
            }
        }
    }
}

/// The whole tree as a Netscape bookmark file: the Bookmarks Menu's contents at the top, then the
/// toolbar and Other Bookmarks as the folders Firefox marks with `PERSONAL_TOOLBAR_FOLDER` and
/// `UNFILED_BOOKMARKS_FOLDER`.
pub fn export_html(conn: &Connection) -> rusqlite::Result<String> {
    let tree = Tree::load(conn)?;
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<!-- This is an automatically generated file.\n     It will be read and overwritten.\n     DO NOT EDIT! -->\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks Menu</H1>\n\n<DL><p>\n",
    );
    html_children(&mut out, &tree, ROOT_MENU, 1);
    for (root, flag) in [(ROOT_TOOLBAR, "PERSONAL_TOOLBAR_FOLDER"), (ROOT_OTHER, "UNFILED_BOOKMARKS_FOLDER")] {
        let (created, modified) = tree.nodes.get(&root).map(|n| (n.created, n.modified)).unwrap_or((0, 0));
        out.push_str(&format!(
            "    <DT><H3 ADD_DATE=\"{}\" LAST_MODIFIED=\"{}\" {flag}=\"true\">{}</H3>\n    <DL><p>\n",
            created / 1000,
            modified / 1000,
            html_escape(&tree.root_title(root))
        ));
        html_children(&mut out, &tree, root, 2);
        out.push_str("    </DL><p>\n");
    }
    out.push_str("</DL>\n");
    Ok(out)
}

enum Item {
    Folder(ParsedFolder),
    Mark(ParsedMark),
    Separator,
}

struct ParsedFolder {
    title: String,
    toolbar: bool,
    unfiled: bool,
    created: Option<i64>,
    modified: Option<i64>,
    items: Vec<Item>,
}

struct ParsedMark {
    url: String,
    title: String,
    keyword: String,
    tags: Vec<String>,
    created: Option<i64>,
    modified: Option<i64>,
}

/// Decodes the entities bookmark files use: the named ones and numeric ones.
fn html_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest.find(';').filter(|e| *e <= 10).and_then(|end| {
            let name = &rest[1..end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix('#')
                    .and_then(|n| match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse::<u32>().ok(),
                    })
                    .and_then(char::from_u32),
            };
            ch.map(|c| (c, end + 1))
        });
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes tags from a text (a title that holds markup).
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Index of the `>` that ends the tag starting at `start`, skipping any inside quoted values.
fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in html[start + 1..].char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(start + 1 + i),
            _ => {}
        }
    }
    None
}

/// `name="value"` pairs of a tag (names lower-cased, values still escaped).
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let chars: Vec<char> = tag.chars().collect();
    let mut i = 0;
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
            i += 1;
        }
        let name_start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' && chars[i] != '/' {
            i += 1;
        }
        if i == name_start {
            break;
        }
        let name: String = chars[name_start..i].iter().collect::<String>().to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        out.push((name, value));
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// A date attribute as milliseconds: files carry seconds (Firefox, Chrome), sometimes milliseconds
/// or microseconds.
fn date_ms(raw: Option<&str>) -> Option<i64> {
    let n: i64 = raw?.trim().parse().ok()?;
    match n {
        n if n <= 0 => None,
        n if n < 100_000_000_000 => Some(n * 1000),
        n if n < 100_000_000_000_000 => Some(n),
        n => Some(n / 1000),
    }
}

struct Frame {
    folder: Option<ParsedFolder>,
    items: Vec<Item>,
}

/// Reads a Netscape bookmark file into nested items. Folders are `<H3>` titles followed by a
/// `<DL>` list; the file's own outermost list is not a folder.
fn parse_html(html: &str) -> Vec<Item> {
    let lower = html.to_ascii_lowercase();
    let mut stack = vec![Frame { folder: None, items: Vec::new() }];
    let mut pending: Option<ParsedFolder> = None;
    let mut too_deep = 0usize;
    let mut i = 0;

    // A heading that never got a list is an empty folder.
    fn flush(pending: &mut Option<ParsedFolder>, stack: &mut [Frame]) {
        if let Some(f) = pending.take() {
            if let Some(top) = stack.last_mut() {
                top.items.push(Item::Folder(f));
            }
        }
    }

    while let Some(rel) = lower[i..].find('<') {
        let start = i + rel;
        if lower[start..].starts_with("<!--") {
            i = lower[start + 4..].find("-->").map(|e| start + 4 + e + 3).unwrap_or(lower.len());
            continue;
        }
        let Some(end) = tag_end(html, start) else { break };
        let tag = &html[start + 1..end];
        let name = lower[start + 1..end].split(|c: char| c.is_whitespace() || c == '/').find(|s| !s.is_empty()).unwrap_or("");
        let closing = lower[start + 1..].starts_with('/');
        i = end + 1;
        match (closing, name) {
            (false, "h3") => {
                flush(&mut pending, &mut stack);
                let close = lower[i..].find("</h3>").map(|c| i + c).unwrap_or(lower.len());
                let attrs = attributes(tag);
                pending = Some(ParsedFolder {
                    title: html_unescape(strip_tags(&html[i..close]).trim()),
                    toolbar: attr(&attrs, "personal_toolbar_folder").is_some_and(|v| v.eq_ignore_ascii_case("true")),
                    unfiled: attr(&attrs, "unfiled_bookmarks_folder").is_some_and(|v| v.eq_ignore_ascii_case("true")),
                    created: date_ms(attr(&attrs, "add_date")),
                    modified: date_ms(attr(&attrs, "last_modified")),
                    items: Vec::new(),
                });
                i = (close + 5).min(lower.len());
            }
            (false, "dl") => {
                // Nesting past any real file (a hostile one could nest a million lists): the lists
                // beyond the limit are flattened into the one at the limit.
                if stack.len() >= 2 * MAX_DEPTH {
                    too_deep += 1;
                    pending = None;
                    continue;
                }
                let folder = pending.take();
                stack.push(Frame { folder, items: Vec::new() });
            }
            (true, "dl") => {
                if too_deep > 0 {
                    too_deep -= 1;
                    continue;
                }
                flush(&mut pending, &mut stack);
                if stack.len() > 1 {
                    let frame = stack.pop().expect("a frame above the base one");
                    let top = stack.last_mut().expect("the base frame");
                    match frame.folder {
                        Some(mut f) => {
                            f.items = frame.items;
                            top.items.push(Item::Folder(f));
                        }
                        // A list without a heading (the file's outermost one): its items belong here.
                        None => top.items.extend(frame.items),
                    }
                }
            }
            (false, "a") => {
                flush(&mut pending, &mut stack);
                let close = lower[i..].find("</a>").map(|c| i + c).unwrap_or(lower.len());
                let attrs = attributes(tag);
                if let Some(href) = attr(&attrs, "href") {
                    let mark = ParsedMark {
                        url: html_unescape(href.trim()),
                        title: html_unescape(strip_tags(&html[i..close]).trim()),
                        keyword: attr(&attrs, "shortcuturl").map(html_unescape).unwrap_or_default(),
                        tags: attr(&attrs, "tags").map(|t| html_unescape(t).split(',').map(str::to_string).collect()).unwrap_or_default(),
                        created: date_ms(attr(&attrs, "add_date")),
                        modified: date_ms(attr(&attrs, "last_modified")),
                    };
                    if let Some(top) = stack.last_mut() {
                        top.items.push(Item::Mark(mark));
                    }
                }
                i = (close + 4).min(lower.len());
            }
            (false, "hr") => {
                flush(&mut pending, &mut stack);
                if let Some(top) = stack.last_mut() {
                    top.items.push(Item::Separator);
                }
            }
            _ => {}
        }
    }
    flush(&mut pending, &mut stack);
    // An unclosed file: close what is still open.
    while stack.len() > 1 {
        let frame = stack.pop().expect("a frame above the base one");
        let top = stack.last_mut().expect("the base frame");
        match frame.folder {
            Some(mut f) => {
                f.items = frame.items;
                top.items.push(Item::Folder(f));
            }
            None => top.items.extend(frame.items),
        }
    }
    stack.pop().map(|f| f.items).unwrap_or_default()
}

/// Turns parsed items into insertable nodes. Links other than http(s) (javascript:, place:, file:)
/// are dropped; so is anything past the depth and size limits.
fn convert_items(items: &[Item], depth: usize, budget: &mut usize) -> Vec<Prepared> {
    let mut out = Vec::new();
    for item in items {
        if *budget == 0 {
            break;
        }
        match item {
            Item::Separator => {
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Separator,
                    title: String::new(),
                    url: String::new(),
                    tags: Vec::new(),
                    keyword: String::new(),
                    spaces: Vec::new(),
                    created: None,
                    modified: None,
                    children: Vec::new(),
                });
            }
            Item::Mark(m) => {
                let Ok(url) = clean_url(&m.url) else { continue };
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Bookmark,
                    title: clean_title(&m.title),
                    url,
                    tags: clean_tags(&m.tags),
                    keyword: clean_keyword(&m.keyword).unwrap_or_default(),
                    spaces: Vec::new(),
                    created: m.created,
                    modified: m.modified,
                    children: Vec::new(),
                });
            }
            Item::Folder(f) => {
                if depth >= MAX_DEPTH {
                    continue;
                }
                *budget -= 1;
                out.push(Prepared {
                    kind: Kind::Folder,
                    title: clean_title(&f.title),
                    url: String::new(),
                    tags: Vec::new(),
                    keyword: String::new(),
                    spaces: Vec::new(),
                    created: f.created,
                    modified: f.modified,
                    children: convert_items(&f.items, depth + 1, budget),
                });
            }
        }
    }
    out
}

/// Imports a Netscape bookmark file into the existing bookmarks. The folder marked as the toolbar
/// merges into the Bookmarks Toolbar, the one marked "unfiled" into Other Bookmarks, and everything
/// else goes into the Bookmarks Menu with its folders. Same-named folders are reused and bookmarks
/// already in their folder are skipped, so importing a file twice adds nothing. Returns the number
/// of new bookmarks.
pub fn import_html(conn: &mut Connection, html: &str) -> rusqlite::Result<usize> {
    let items = parse_html(html);
    let mut budget = MAX_NODES;
    let (mut toolbar, mut menu, mut other) = (Vec::new(), Vec::new(), Vec::new());
    let mut loose: Vec<&Item> = Vec::new();
    for item in &items {
        match item {
            Item::Folder(f) if f.toolbar => toolbar.extend(convert_items(&f.items, 1, &mut budget)),
            Item::Folder(f) if f.unfiled => other.extend(convert_items(&f.items, 1, &mut budget)),
            _ => loose.push(item),
        }
    }
    for item in loose {
        menu.extend(convert_items(std::slice::from_ref(item), 0, &mut budget));
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut ctx = Ctx { now: now_ms(), steal_keywords: false, merge: true, added: 0 };
    for (root, nodes) in [(ROOT_TOOLBAR, &toolbar), (ROOT_MENU, &menu), (ROOT_OTHER, &other)] {
        // Separators only make sense when the folder starts out empty.
        let fresh = children_ids(&tx, root)?.is_empty();
        for p in nodes {
            insert_one(&tx, root, p, &mut ctx, fresh)?;
        }
    }
    tx.commit()?;
    Ok(ctx.added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::bookmarks::{add, list, NewNode};
    use crate::bookmarks::tests::{folder, mark, mem, positions_are_contiguous, sample_tree, shape, titles};

    #[test]
    fn html_export_and_import_round_trip() {
        let mut a = mem();
        sample_tree(&mut a);
        let html = export_html(&a).unwrap();
        assert!(html.contains("PERSONAL_TOOLBAR_FOLDER=\"true\">Bookmarks Toolbar</H3>"));
        assert!(html.contains("UNFILED_BOOKMARKS_FOLDER=\"true\">Other Bookmarks</H3>"));
        assert!(html.contains("SHORTCUTURL=\"n\""));
        assert!(html.contains("TAGS=\"news,daily\""));
        assert!(html.contains("<HR>"));
        assert!(html.contains("News &amp; &quot;views&quot;"));
        assert!(html.contains("a=1&amp;b=2"));
        let mut b = mem();
        let n = import_html(&mut b, &html).unwrap();
        assert_eq!(n, 5);
        assert_eq!(shape(&a), shape(&b));
        let toolbar = titles(&b, ROOT_TOOLBAR);
        assert_eq!(toolbar, ["News & \"views\"", "", "Dev"], "the separator comes back in place");
        let news = list(&b).unwrap().into_iter().find(|n| n.keyword == "n").unwrap();
        assert_eq!(news.tags, ["news", "daily"]);
        // Importing the same file again adds nothing.
        assert_eq!(import_html(&mut b, &html).unwrap(), 0);
        assert_eq!(shape(&a), shape(&b));
        positions_are_contiguous(&b);
    }

    #[test]
    fn firefox_and_chrome_files_land_in_the_right_places() {
        let firefox = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks Menu</H1>
<DL><p>
    <DT><H3 ADD_DATE="1600000000" LAST_MODIFIED="1600000001">Mozilla Firefox</H3>
    <DL><p>
        <DT><A HREF="https://www.mozilla.org/firefox/help/" ADD_DATE="1600000002" LAST_MODIFIED="1600000003" TAGS="help,firefox">Get Help</A>
        <DD>A description line that is ignored
    </DL><p>
    <DT><H3 ADD_DATE="1" LAST_MODIFIED="2" PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Toolbar</H3>
    <DL><p>
        <DT><A HREF="https://toolbar.example/" ADD_DATE="1">On the bar</A>
        <DT><A HREF="place:sort=8&maxResults=10" ADD_DATE="1">A smart folder</A>
        <DT><A HREF="javascript:alert(1)">Bookmarklet</A>
    </DL><p>
    <DT><H3 ADD_DATE="1" LAST_MODIFIED="2" UNFILED_BOOKMARKS_FOLDER="true">Other Bookmarks</H3>
    <DL><p>
        <DT><A HREF="https://other.example/">Unsorted</A>
        <DT><H3>Empty folder</H3>
        <DL><p>
        </DL><p>
    </DL><p>
</DL>"#;
        let mut c = mem();
        assert_eq!(import_html(&mut c, firefox).unwrap(), 3);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["On the bar"]);
        assert_eq!(titles(&c, ROOT_OTHER), ["Unsorted", "Empty folder"]);
        assert_eq!(titles(&c, ROOT_MENU), ["Mozilla Firefox"]);
        let help = list(&c).unwrap().into_iter().find(|n| n.title == "Get Help").unwrap();
        assert_eq!(help.tags, ["help", "firefox"]);
        assert_eq!(help.created, 1_600_000_002_000);
        assert_eq!(help.modified, 1_600_000_003_000);

        let chrome = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
    <DT><H3 ADD_DATE="1700000000" LAST_MODIFIED="1700000000" PERSONAL_TOOLBAR_FOLDER="true">Bookmarks bar</H3>
    <DL><p>
        <DT><A HREF="https://bar.example/" ADD_DATE="1700000001" ICON="data:image/png;base64,AAAA">Bar item</A>
        <DT><H3 ADD_DATE="1700000002" LAST_MODIFIED="1700000002">Recipes</H3>
        <DL><p>
            <DT><A HREF="https://pasta.example/?a=1&amp;b=2" ADD_DATE="1700000003">Pasta &#39;n&#x27; more</A>
        </DL><p>
    </DL><p>
    <DT><H3 ADD_DATE="1700000004" LAST_MODIFIED="1700000004">Other bookmarks</H3>
    <DL><p>
        <DT><A HREF="HTTPS://UPPER.example/" ADD_DATE="1700000005">Upper case scheme</A>
    </DL><p>
</DL><p>"#;
        let mut c = mem();
        assert_eq!(import_html(&mut c, chrome).unwrap(), 3);
        assert_eq!(titles(&c, ROOT_TOOLBAR), ["Bar item", "Recipes"]);
        assert_eq!(titles(&c, ROOT_MENU), ["Other bookmarks"]);
        let pasta = list(&c).unwrap().into_iter().find(|n| n.url.contains("pasta")).unwrap();
        assert_eq!(pasta.url, "https://pasta.example/?a=1&b=2");
        assert_eq!(pasta.title, "Pasta 'n' more");
    }

    #[test]
    fn importing_merges_into_existing_folders_without_touching_keywords() {
        let mut c = mem();
        add(&mut c, ROOT_TOOLBAR, None, &[NewNode { keyword: "k".into(), ..mark("mine", "https://mine.example/") }, folder("Dev", vec![mark("kept", "https://kept.example/")])]).unwrap();
        let html = r#"<DL><p>
            <DT><H3 PERSONAL_TOOLBAR_FOLDER="true">Bookmarks Toolbar</H3>
            <DL><p>
              <DT><A HREF="https://mine.example/" SHORTCUTURL="x">same page, same folder</A>
              <DT><A HREF="https://new.example/" SHORTCUTURL="k">takes a used keyword</A>
              <DT><H3>Dev</H3>
              <DL><p>
                <DT><A HREF="https://kept.example/">duplicate</A>
                <DT><A HREF="https://added.example/">added</A>
              </DL><p>
            </DL><p>
        </DL>"#;
        assert_eq!(import_html(&mut c, html).unwrap(), 2);
        let all = list(&c).unwrap();
        assert_eq!(all.iter().filter(|n| n.title == "Dev").count(), 1, "the existing folder is reused");
        let dev = all.iter().find(|n| n.title == "Dev").unwrap().id;
        assert_eq!(titles(&c, dev), ["kept", "added"]);
        assert_eq!(all.iter().find(|n| n.title == "mine").unwrap().keyword, "k");
        assert_eq!(all.iter().find(|n| n.title == "takes a used keyword").unwrap().keyword, "");
        assert!(import_html(&mut c, "not a bookmarks file at all").unwrap() == 0);
    }

    #[test]
    fn entities_and_odd_markup_are_read_tolerantly() {
        assert_eq!(html_unescape("a &amp; b &lt;c&gt; &quot;d&quot; &#65;&#x42; &unknown; &"), "a & b <c> \"d\" AB &unknown; &");
        assert_eq!(strip_tags("Deep <b>bold</b> text"), "Deep bold text");
        let attrs = attributes("A HREF=\"https://x.example/?q=a>b\" add_date=5 Tags='x,y' hidden");
        assert_eq!(attr(&attrs, "href"), Some("https://x.example/?q=a>b"));
        assert_eq!(attr(&attrs, "add_date"), Some("5"));
        assert_eq!(attr(&attrs, "tags"), Some("x,y"));
        assert_eq!(attr(&attrs, "hidden"), Some(""));
        assert_eq!(date_ms(Some("1700000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("1700000000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("1700000000000000")), Some(1_700_000_000_000));
        assert_eq!(date_ms(Some("0")), None);
        assert_eq!(date_ms(Some("x")), None);
        // A '>' inside a quoted address doesn't end the tag.
        let mut c = mem();
        let html = "<DL><DT><A HREF=\"https://x.example/?q=a>b\">Quoted</A><DT><a href='https://y.example/'>single quotes</a></DL>";
        assert_eq!(import_html(&mut c, html).unwrap(), 2);
        assert_eq!(titles(&c, ROOT_MENU), ["Quoted", "single quotes"]);
    }

    #[test]
    fn hostile_nesting_in_a_file_is_contained() {
        let mut c = mem();
        let mut html = String::from("<DL><p>");
        for _ in 0..20_000 {
            html.push_str("<DT><H3>f</H3><DL><p>");
        }
        html.push_str("<DT><A HREF=\"https://deep.example/\">deep</A>");
        for _ in 0..20_000 {
            html.push_str("</DL><p>");
        }
        html.push_str("</DL>");
        // It neither overflows the stack nor creates more than the allowed depth.
        let n = import_html(&mut c, &html).unwrap();
        assert!(n <= 1, "the bookmark past the limit may be dropped, never duplicated");
        let depth = |c: &Connection| {
            let all = list(c).unwrap();
            let by: HashMap<i64, Option<i64>> = all.iter().map(|n| (n.id, n.parent)).collect();
            all.iter().map(|n| { let (mut d, mut cur) = (0, n.parent); while let Some(p) = cur { d += 1; cur = by[&p]; } d }).max().unwrap()
        };
        assert!(depth(&c) <= MAX_DEPTH + 2, "depth {}", depth(&c));
    }
}
