// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The production guide's Keycloak recipe, read from the book page as the
//! page prints it, so the recipe a test applies and the recipe an operator
//! copies cannot drift apart.
//!
//! The section `### An issuer recipe: Keycloak` of
//! `website/book/src/operate/production.md` carries the `kcadm.sh` commands
//! in two `sh` blocks, each protocol mapper file in a `json` block that the
//! paragraph before it names, and the gateway's `[auth]` table in a `toml`
//! block. [`Recipe::from_page`] reads all three kinds and refuses a page
//! whose commands and files disagree.

/// The book page that carries the recipe.
pub const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/production.md"
);

/// The heading of the recipe's section.
const HEADING: &str = "### An issuer recipe: Keycloak";

/// The placeholder address of Keycloak on the page, which a test replaces
/// with the address of the Keycloak it starts.
pub const PAGE_SERVER: &str = "https://idp.example.org";

/// The page's recipe could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RecipeError {
    /// The page has no section with the recipe's heading.
    #[error("the page has no section headed {HEADING:?}")]
    NoSection,
    /// A fenced block of the section is never closed.
    #[error("a fenced block of the recipe is never closed")]
    Unclosed,
    /// The section carries no `sh` block.
    #[error("the recipe carries no kcadm.sh commands")]
    NoCommands,
    /// A `json` block follows no paragraph that names its file.
    #[error("a json block of the recipe follows no paragraph naming its file")]
    UnnamedFile,
    /// The commands name a file the section does not carry.
    #[error("the recipe's commands read {0}, which the page does not carry")]
    MissingFile(String),
    /// The section carries a file no command reads.
    #[error("the page carries {0}, which no command of the recipe reads")]
    UnreadFile(String),
    /// The section carries no `[auth]` table for `ferrofed.toml`.
    #[error("the recipe carries no [auth] table for ferrofed.toml")]
    NoAuth,
    /// The commands do not name [`PAGE_SERVER`] exactly once as the server.
    #[error("the recipe names --server {PAGE_SERVER} {0} times, where a test replaces it once")]
    Server(usize),
}

/// One file the recipe's commands read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeFile {
    /// The file name the commands use.
    pub name: String,
    /// The content, as the page prints it.
    pub content: String,
}

/// The recipe as the page prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    commands: String,
    files: Vec<RecipeFile>,
    auth: String,
}

impl Recipe {
    /// Reads the recipe from `page`, the text of the production guide.
    ///
    /// # Errors
    ///
    /// Returns a [`RecipeError`] naming what the page lacks, or the file
    /// its commands and its blocks disagree on.
    pub fn from_page(page: &str) -> Result<Self, RecipeError> {
        let section = section(page).ok_or(RecipeError::NoSection)?;
        let mut commands = Vec::new();
        let mut files = Vec::new();
        let mut auth = None;
        let mut named: Option<String> = None;
        let mut opening = true;
        let mut lines = section.lines();
        while let Some(line) = lines.next() {
            let Some(info) = line.strip_prefix("```") else {
                if line.trim().is_empty() {
                    opening = true;
                } else if opening {
                    named = file_named(line);
                    opening = false;
                }
                continue;
            };
            let block = fenced(&mut lines).ok_or(RecipeError::Unclosed)?;
            opening = true;
            match info.trim() {
                "sh" => commands.push(block),
                "json" => files.push(RecipeFile {
                    name: named.take().ok_or(RecipeError::UnnamedFile)?,
                    content: block,
                }),
                "toml" if block.starts_with("# ferrofed.toml\n[auth]") => auth = Some(block),
                _ => {}
            }
        }
        if commands.is_empty() {
            return Err(RecipeError::NoCommands);
        }
        let commands = commands.join("\n");
        let read = files_read(&commands);
        if let Some(missing) = read
            .iter()
            .find(|name| !files.iter().any(|file| file.name == **name))
        {
            return Err(RecipeError::MissingFile(missing.clone()));
        }
        if let Some(unread) = files.iter().find(|file| !read.contains(&file.name)) {
            return Err(RecipeError::UnreadFile(unread.name.clone()));
        }
        Ok(Self {
            commands,
            files,
            auth: auth.ok_or(RecipeError::NoAuth)?,
        })
    }

    /// Returns the `kcadm.sh` commands as the page prints them, the blocks
    /// joined in order.
    #[must_use]
    pub fn commands(&self) -> &str {
        &self.commands
    }

    /// Returns the commands with `server` in place of [`PAGE_SERVER`] as the
    /// address `kcadm.sh config credentials` logs in to.
    ///
    /// # Errors
    ///
    /// Returns [`RecipeError::Server`] unless the commands name
    /// `--server` [`PAGE_SERVER`] exactly once.
    pub fn commands_against(&self, server: &str) -> Result<String, RecipeError> {
        let needle = format!("--server {PAGE_SERVER} ");
        match self.commands.matches(&needle).count() {
            1 => Ok(self
                .commands
                .replacen(&needle, &format!("--server {server} "), 1)),
            other => Err(RecipeError::Server(other)),
        }
    }

    /// Returns the files the commands read, in the order of the page.
    #[must_use]
    pub fn files(&self) -> &[RecipeFile] {
        &self.files
    }

    /// Returns the `[auth]` table of `ferrofed.toml` with `origin` in place
    /// of [`PAGE_SERVER`] in the issuer and its key set location.
    #[must_use]
    pub fn auth_against(&self, origin: &str) -> String {
        self.auth.replace(PAGE_SERVER, origin)
    }
}

/// The text of the recipe's section: from its heading to the next heading
/// of the same or a higher level.
fn section(page: &str) -> Option<String> {
    let mut lines = page.lines().skip_while(|line| line.trim_end() != HEADING);
    lines.next()?;
    let body: Vec<&str> = lines
        .take_while(|line| !(line.starts_with("## ") || line.starts_with("### ")))
        .collect();
    Some(body.join("\n"))
}

/// The lines of a fenced block up to its closing fence, or `None` when the
/// block is never closed.
fn fenced<'a>(lines: &mut impl Iterator<Item = &'a str>) -> Option<String> {
    let mut block = String::new();
    for line in lines.by_ref() {
        if line.trim_end() == "```" {
            return Some(block);
        }
        block.push_str(line);
        block.push('\n');
    }
    None
}

/// The file a paragraph whose first line is `line` introduces: the
/// backticked `*.json` name it opens with.
fn file_named(line: &str) -> Option<String> {
    let rest = line.strip_prefix('`')?;
    let (name, _) = rest.split_once('`')?;
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        .then(|| name.to_owned())
}

/// Every file the commands read with `-f`, in order, each once.
fn files_read(commands: &str) -> Vec<String> {
    let mut read: Vec<String> = Vec::new();
    let mut words = commands.split_whitespace();
    while let Some(word) = words.next() {
        if word == "-f"
            && let Some(name) = words.next()
            && !read.iter().any(|seen| seen == name)
        {
            read.push(name.to_owned());
        }
    }
    read
}

#[cfg(test)]
mod tests {
    use super::{PAGE_SERVER, Recipe, RecipeError};

    const PAGE: &str = "# Guide\n\n### An issuer recipe: Keycloak\n\nRun it:\n\n```sh\nkcadm.sh config credentials --server https://idp.example.org --realm master --user admin\nkcadm.sh create x -f a.json\n```\n\n`a.json` names a mapper:\n\n```json\n{\"name\": \"a\"}\n```\n\n```toml\n# ferrofed.toml\n[auth]\nissuer = \"https://idp.example.org/realms/r\"\n```\n\n## 5. Next\n\n```json\n{}\n```\n";

    #[test]
    fn a_page_yields_its_commands_files_and_auth_table() {
        let recipe = Recipe::from_page(PAGE).expect("the page carries a recipe");
        assert_eq!(1, recipe.files().len());
        assert_eq!("a.json", recipe.files()[0].name);
        assert_eq!("{\"name\": \"a\"}\n", recipe.files()[0].content);
        assert!(
            recipe
                .commands_against("http://kc:8080")
                .expect("one server")
                .contains("--server http://kc:8080 --realm")
        );
        assert_eq!(
            "# ferrofed.toml\n[auth]\nissuer = \"http://127.0.0.1:1/realms/r\"\n",
            recipe.auth_against("http://127.0.0.1:1")
        );
        assert!(!recipe.auth_against("x").contains(PAGE_SERVER));
    }

    #[test]
    fn a_file_the_commands_read_must_be_on_the_page() {
        let page = PAGE.replace("-f a.json", "-f a.json -f b.json");
        assert_eq!(
            Err(RecipeError::MissingFile("b.json".to_owned())),
            Recipe::from_page(&page)
        );
    }

    #[test]
    fn a_file_on_the_page_must_be_read_by_a_command() {
        let page = PAGE.replace("-f a.json", "");
        assert_eq!(
            Err(RecipeError::UnreadFile("a.json".to_owned())),
            Recipe::from_page(&page)
        );
    }

    #[test]
    fn a_json_block_needs_a_paragraph_naming_it() {
        let page = PAGE.replace("`a.json` names a mapper:", "A mapper:");
        assert_eq!(Err(RecipeError::UnnamedFile), Recipe::from_page(&page));
    }

    #[test]
    fn a_page_without_the_section_is_refused() {
        assert_eq!(Err(RecipeError::NoSection), Recipe::from_page("# Guide\n"));
    }
}
