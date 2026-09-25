// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Render CSV from stdin as an A4-landscape PDF table on stdout.
//! Builds a pandas-style HTML table and converts it with `weasyprint - -`.

use std::io::{self, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio, exit};

const CSS: &str = "
  @page { size: A4 landscape; margin: 1cm; }
  table { border-collapse: collapse; font-family: sans-serif; font-size: 9pt; }
  th, td { border: 1px solid #999; padding: 4px 6px; }
  th { background: #eee; }
  tr:nth-child(even) { background: #f7f7f7; }
";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Missing/empty cells render as "NaN", like pandas' default `na_rep`.
fn cell(s: Option<&str>) -> String {
    match s {
        Some(v) if !v.is_empty() => escape(v),
        _ => "NaN".into(),
    }
}

/// `DataFrame.to_html(index=False)`-shaped markup preceded by the stylesheet.
fn to_html(input: impl io::Read) -> Result<String, String> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_reader(input);
    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    if headers.is_empty() {
        return Err("No columns to parse from file".into());
    }
    let mut html = format!("<style>{CSS}</style>\n");
    html.push_str("<table border=\"1\" class=\"dataframe\">\n  <thead>\n    <tr style=\"text-align: right;\">\n");
    for h in &headers {
        html.push_str(&format!("      <th>{}</th>\n", escape(h)));
    }
    html.push_str("    </tr>\n  </thead>\n  <tbody>\n");
    for rec in rdr.records() {
        let rec = rec.map_err(|e| e.to_string())?;
        if rec.len() > headers.len() {
            return Err(format!(
                "Error tokenizing data. Expected {} fields, saw {}",
                headers.len(),
                rec.len()
            ));
        }
        html.push_str("    <tr>\n");
        for i in 0..headers.len() {
            html.push_str(&format!("      <td>{}</td>\n", cell(rec.get(i))));
        }
        html.push_str("    </tr>\n");
    }
    html.push_str("  </tbody>\n</table>");
    Ok(html)
}

fn main() {
    let html = match to_html(io::stdin().lock()) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("csv2pdf: {e}");
            exit(1);
        }
    };
    let mut child = match Command::new("weasyprint")
        .args(["-", "-"])
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("csv2pdf: weasyprint: {e}");
            exit(if e.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            });
        }
    };
    if let Some(mut stdin) = child.stdin.take()
        && let Err(e) = stdin.write_all(html.as_bytes())
    {
        eprintln!("csv2pdf: weasyprint: {e}");
    }
    let rc = match child.wait() {
        Ok(s) => s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
        Err(e) => {
            eprintln!("csv2pdf: weasyprint: {e}");
            1
        }
    };
    exit(rc);
}

#[cfg(test)]
mod tests {
    use super::to_html;

    #[test]
    fn pandas_like_table() {
        let html = to_html("a,b\n1,<x>\n2\n".as_bytes()).unwrap();
        assert!(html.starts_with("<style>"));
        assert!(html.contains("<table border=\"1\" class=\"dataframe\">"));
        assert!(html.contains("      <th>a</th>\n      <th>b</th>\n"));
        assert!(html.contains("      <td>1</td>\n      <td>&lt;x&gt;</td>\n"));
        assert!(html.contains("      <td>2</td>\n      <td>NaN</td>\n"));
    }

    #[test]
    fn rejects_empty_and_ragged() {
        assert!(to_html("".as_bytes()).is_err());
        assert!(to_html("a\n1,2\n".as_bytes()).is_err());
    }
}
