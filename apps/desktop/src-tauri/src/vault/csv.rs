//! CSV for password import and export. Reads the exports of Chrome, Edge, Brave, Opera, Vivaldi,
//! Firefox, Safari, Bitwarden, 1Password, LastPass, Dashlane, Proton Pass and KeePass(XC) by
//! matching their column names; writes Chrome's format (`name,url,username,password,note`),
//! which all of them can import.

use zeroize::Zeroizing;

/// One credential read from a file, before it is checked against the vault.
pub struct ImportRow {
    /// The address as written in the file.
    pub url: String,
    pub username: String,
    pub password: Zeroizing<String>,
    pub label: String,
    pub notes: Zeroizing<String>,
}

/// Splits CSV text into records (RFC 4180: quoted fields, doubled quotes, CRLF or LF, newlines
/// inside quotes). Tolerates a byte-order mark and a missing final newline.
pub fn parse(text: &str) -> Vec<Vec<Zeroizing<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row: Vec<Zeroizing<String>> = Vec::new();
    let mut field = Zeroizing::new(String::new());
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => row.push(std::mem::replace(&mut field, Zeroizing::new(String::new()))),
            '\r' => {}
            '\n' => {
                row.push(std::mem::replace(&mut field, Zeroizing::new(String::new())));
                if !(row.len() == 1 && row[0].is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
                any = false;
            }
            _ => field.push(c),
        }
    }
    if any {
        row.push(field);
        if !(row.len() == 1 && row[0].is_empty()) {
            rows.push(row);
        }
    }
    rows
}

fn column(header: &[String], names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| header.iter().position(|h| h == n))
}

/// Reads a password export. Errors when the file has no address or password column.
pub fn read_export(text: &str) -> Result<Vec<ImportRow>, String> {
    let mut records = parse(text).into_iter();
    let header: Vec<String> = records
        .next()
        .ok_or("The file is empty.")?
        .iter()
        .map(|h| h.trim().trim_matches('"').to_ascii_lowercase())
        .collect();
    let url = column(&header, &["url", "login_uri", "login url", "website", "web site", "origin", "uri", "hostname", "address"]);
    let password = column(&header, &["password", "login_password", "pass"]);
    let (Some(url), Some(password)) = (url, password) else {
        return Err("This doesn't look like a password export: it needs a URL column and a password column.".into());
    };
    let username = column(&header, &["username", "login_username", "user name", "login", "user", "email", "e-mail", "login name"]);
    let label = column(&header, &["name", "title", "account", "label"]);
    let notes = column(&header, &["note", "notes", "extra", "comments", "comment"]);
    let get = |r: &[Zeroizing<String>], i: Option<usize>| -> String { i.and_then(|i| r.get(i)).map(|s| s.to_string()).unwrap_or_default() };
    let mut out = Vec::new();
    for r in records {
        // Bitwarden joins several addresses with commas inside one field: use the first.
        let first_url = get(&r, Some(url)).split([',', '\n']).next().unwrap_or("").trim().to_string();
        out.push(ImportRow {
            url: first_url,
            username: get(&r, username).trim().to_string(),
            password: Zeroizing::new(get(&r, Some(password))),
            label: get(&r, label).trim().to_string(),
            notes: Zeroizing::new(get(&r, notes)),
        });
    }
    Ok(out)
}

fn quote(out: &mut String, value: &str) {
    if value.contains([',', '"', '\n', '\r']) || value.starts_with(' ') || value.ends_with(' ') {
        out.push('"');
        out.push_str(&value.replace('"', "\"\""));
        out.push('"');
    } else {
        out.push_str(value);
    }
}

/// Writes rows of (name, url, username, password, note) in Chrome's format.
pub fn write_export(rows: &[(String, String, String, Zeroizing<String>, Zeroizing<String>)]) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::from("name,url,username,password,note\r\n"));
    for (name, url, user, pass, note) in rows {
        for (i, v) in [name.as_str(), url, user, pass, note].iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            quote(&mut out, v);
        }
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_and_newlines() {
        let rows = parse("a,b,c\r\n\"x, y\",\"say \"\"hi\"\"\",\"multi\nline\"\n\nlast,,\n");
        assert_eq!(rows.len(), 3);
        assert_eq!(*rows[1][0], "x, y");
        assert_eq!(*rows[1][1], "say \"hi\"");
        assert_eq!(*rows[1][2], "multi\nline");
        assert_eq!(rows[2].len(), 3);
        assert_eq!(*rows[2][1], "");
    }

    #[test]
    fn reads_chrome_firefox_and_bitwarden() {
        let chrome = read_export("\u{feff}name,url,username,password,note\nexample.com,https://example.com/login,ann,pw1,hi\n").unwrap();
        assert_eq!(chrome[0].url, "https://example.com/login");
        assert_eq!(chrome[0].username, "ann");
        assert_eq!(*chrome[0].password, "pw1");
        assert_eq!(*chrome[0].notes, "hi");

        let firefox = read_export("\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\"\n\"https://a.org\",\"bob\",\"p,w\",,\"https://a.org\",\"{1}\"\n").unwrap();
        assert_eq!(firefox[0].username, "bob");
        assert_eq!(*firefox[0].password, "p,w");

        let bw = read_export("folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,,login,Mail,,,0,\"https://mail.b.com,https://b.com\",cy,secret,\n").unwrap();
        assert_eq!(bw[0].url, "https://mail.b.com");
        assert_eq!(bw[0].label, "Mail");
        assert_eq!(*bw[0].password, "secret");
    }

    #[test]
    fn rejects_unrelated_csv() {
        assert!(read_export("a,b\n1,2\n").is_err());
        assert!(read_export("").is_err());
    }

    #[test]
    fn export_round_trips() {
        let rows = vec![(
            "Example".to_string(),
            "https://example.com".to_string(),
            "a\"b".to_string(),
            Zeroizing::new("p,w\nx".to_string()),
            Zeroizing::new(String::new()),
        )];
        let text = write_export(&rows);
        let back = read_export(&text).unwrap();
        assert_eq!(back[0].username, "a\"b");
        assert_eq!(*back[0].password, "p,w\nx");
        assert_eq!(back[0].label, "Example");
    }
}
