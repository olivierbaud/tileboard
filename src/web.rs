//! The frontend files. They are built into the binary, so a deploy is one file.
//! `--web-dir` reads them from a folder instead, so a frontend change shows on reload.

use std::borrow::Cow;
use std::path::PathBuf;

pub struct WebFile {
    /// The address the browser asks for.
    pub route: &'static str,
    /// The file name in `web/`.
    pub name: &'static str,
    pub content_type: &'static str,
    built_in: &'static str,
}

/// Every file the dashboard serves. Nothing outside this list can be asked for.
pub const FILES: &[WebFile] = &[
    WebFile {
        route: "/",
        name: "index.html",
        content_type: "text/html; charset=utf-8",
        built_in: include_str!("../web/index.html"),
    },
    WebFile {
        route: "/app.js",
        name: "app.js",
        content_type: "text/javascript; charset=utf-8",
        built_in: include_str!("../web/app.js"),
    },
    WebFile {
        route: "/style.css",
        name: "style.css",
        content_type: "text/css; charset=utf-8",
        built_in: include_str!("../web/style.css"),
    },
];

pub struct Web {
    /// `None` serves the built-in files.
    dir: Option<PathBuf>,
}

impl Web {
    pub fn built_in() -> Web {
        Web { dir: None }
    }

    pub fn from_dir(dir: PathBuf) -> Web {
        Web { dir: Some(dir) }
    }

    /// The text of one of [`FILES`].
    pub fn read(&self, file: &WebFile) -> std::io::Result<Cow<'static, str>> {
        match &self.dir {
            None => Ok(Cow::Borrowed(file.built_in)),
            Some(dir) => std::fs::read_to_string(dir.join(file.name)).map(Cow::Owned),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_files_are_there() {
        let web = Web::built_in();
        for file in FILES {
            assert!(!web.read(file).unwrap().is_empty(), "{}", file.name);
        }
    }

    #[test]
    fn a_web_dir_is_read_from_disk() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web");
        let (from_disk, built_in) = (Web::from_dir(dir), Web::built_in());
        for file in FILES {
            // Line endings can differ between the checkout and the built-in copy.
            let normal = |text: Cow<str>| text.replace("\r\n", "\n");
            assert_eq!(
                normal(from_disk.read(file).unwrap()),
                normal(built_in.read(file).unwrap()),
                "{}",
                file.name
            );
        }
        let missing = Web::from_dir(PathBuf::from("/no/such/folder"));
        assert!(missing.read(&FILES[0]).is_err());
    }

    #[test]
    fn the_page_loads_nothing_from_outside() {
        // The page's Content-Security-Policy only allows files from the dashboard itself.
        let page = Web::built_in().read(&FILES[0]).unwrap();
        assert!(
            !page.contains("http://") && !page.contains("https://"),
            "index.html points outside"
        );
        assert!(
            !page.contains("<script>") && !page.contains("style="),
            "index.html has inline script or style"
        );
    }
}
