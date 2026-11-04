// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The import graph between the top-level modules of one crate.
//!
//! The top-level modules are the `mod` items of `src/lib.rs`. A module
//! imports another when one of its files names it through `crate::` (a path,
//! a `use` tree with its groups, or the tokens of a macro or an attribute),
//! or through `super::` in a file directly under `src/`. Test code is left
//! out: an item under a `#[cfg(...)]` that names `test`, a module only such
//! an item declares, and a file under a `tests` directory.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use proc_macro2::{TokenStream, TokenTree};
use syn::visit::{self, Visit};

/// Every edge `(from, to)` between two top-level modules of the crate whose
/// source directory is `src`, sorted.
///
/// # Errors
///
/// Returns an error when a file cannot be read or does not parse as Rust.
pub(crate) fn edges(src: &Path) -> Result<BTreeSet<(String, String)>> {
    let root = parse(&src.join("lib.rs"))?;
    let modules: Vec<(String, Option<&Vec<syn::Item>>)> = root
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(module) if !is_test(&module.attrs) => Some((
                module.ident.to_string(),
                module.content.as_ref().map(|(_, items)| items),
            )),
            _ => None,
        })
        .collect();
    let top_level: BTreeSet<String> = modules.iter().map(|(name, _)| name.clone()).collect();
    let mut edges = BTreeSet::new();
    for (name, inline) in &modules {
        let mut named = BTreeSet::new();
        match inline {
            Some(items) => {
                let mut visitor = Visitor::new(true);
                for item in *items {
                    visitor.visit_item(item);
                }
                named.extend(visitor.named);
            }
            None => {
                if let Some(file) = module_file(src, name) {
                    walk(&file, true, &mut named)?;
                }
            }
        }
        edges.extend(
            named
                .into_iter()
                .filter(|target| target != name && top_level.contains(target))
                .map(|target| (name.clone(), target)),
        );
    }
    Ok(edges)
}

/// The pairs of `edges` that point both ways, each once, sorted.
pub(crate) fn mutual(edges: &BTreeSet<(String, String)>) -> Vec<(String, String)> {
    edges
        .iter()
        .filter(|(from, to)| from < to && edges.contains(&(to.clone(), from.clone())))
        .cloned()
        .collect()
}

/// Adds to `named` every top-level name the module file `file` and the
/// files of its non-test child modules name; `top` is true for a file
/// directly under `src/`, where `super::` names the crate root.
fn walk(file: &Path, top: bool, named: &mut BTreeSet<String>) -> Result<()> {
    let parsed = parse(file)?;
    let mut visitor = Visitor::new(top);
    visitor.visit_file(&parsed);
    named.extend(visitor.named);
    let dir = module_dir(file);
    for child in visitor.children {
        if let Some(child_file) = module_file(&dir, &child) {
            walk(&child_file, false, named)?;
        }
    }
    Ok(())
}

/// Reads and parses the Rust file `file`.
fn parse(file: &Path) -> Result<syn::File> {
    let text = fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    syn::parse_file(&text).with_context(|| format!("parsing {}", file.display()))
}

/// The file of module `name` declared in `dir`: `name.rs` or `name/mod.rs`.
fn module_file(dir: &Path, name: &str) -> Option<PathBuf> {
    [
        dir.join(format!("{name}.rs")),
        dir.join(name).join("mod.rs"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

/// The directory the out-of-line modules a file declares live in: its own
/// directory for a `lib.rs` or a `mod.rs`, else the directory named after it.
fn module_dir(file: &Path) -> PathBuf {
    let parent = file.parent().unwrap_or(Path::new(""));
    match file.file_name().and_then(|name| name.to_str()) {
        Some("lib.rs" | "mod.rs") => parent.to_path_buf(),
        _ => file.with_extension(""),
    }
}

/// Whether `attrs` hold a `#[cfg(...)]` whose predicate names `test` outside
/// a `not(...)`, which leaves the item out of every non-test build.
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .meta
                .require_list()
                .is_ok_and(|list| names_test(list.tokens.clone()))
    })
}

/// Whether the predicate `tokens` names `test` other than under `not`.
fn names_test(tokens: TokenStream) -> bool {
    let mut previous_not = false;
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) if ident == "test" => return true,
            TokenTree::Ident(ident) => previous_not = ident == "not",
            TokenTree::Group(group) => {
                if !previous_not && names_test(group.stream()) {
                    return true;
                }
                previous_not = false;
            }
            TokenTree::Punct(_) | TokenTree::Literal(_) => {}
        }
    }
    false
}

/// The visitor that gathers the top-level names one file uses.
struct Visitor {
    /// Whether the file sits directly under `src/`.
    top: bool,
    /// The names of the top-level modules the file reaches.
    named: BTreeSet<String>,
    /// The out-of-line child modules the file declares outside tests.
    children: Vec<String>,
}

impl Visitor {
    fn new(top: bool) -> Self {
        Self {
            top,
            named: BTreeSet::new(),
            children: Vec::new(),
        }
    }

    /// Records the name after a leading `crate`, or a leading `super` in a
    /// top file, of the segments `first` and `second`.
    fn root_name(&mut self, first: &str, second: Option<String>) {
        if first == "crate" || (self.top && first == "super") {
            self.named.extend(second);
        }
    }

    /// Records every `crate::name`, and `super::name` in a top file, the
    /// tokens `stream` spell, inside their groups too.
    fn tokens(&mut self, stream: TokenStream) {
        let trees: Vec<TokenTree> = stream.into_iter().collect();
        for (at, tree) in trees.iter().enumerate() {
            match tree {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Ident(ident) => {
                    let colons = matches!(
                        (trees.get(at + 1), trees.get(at + 2)),
                        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                            if a.as_char() == ':' && b.as_char() == ':'
                    );
                    if let (true, Some(TokenTree::Ident(next))) = (colons, trees.get(at + 3)) {
                        self.root_name(&ident.to_string(), Some(next.to_string()));
                    }
                }
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }

    /// Records the first names under the `use` subtree `tree`.
    fn use_names(&mut self, tree: &syn::UseTree) {
        match tree {
            syn::UseTree::Path(path) => {
                self.named.insert(path.ident.to_string());
            }
            syn::UseTree::Name(name) => {
                self.named.insert(name.ident.to_string());
            }
            syn::UseTree::Rename(rename) => {
                self.named.insert(rename.ident.to_string());
            }
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.use_names(item);
                }
            }
            syn::UseTree::Glob(_) => {}
        }
    }
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attrs = match item {
            syn::Item::Const(i) => &i.attrs,
            syn::Item::Enum(i) => &i.attrs,
            syn::Item::ExternCrate(i) => &i.attrs,
            syn::Item::Fn(i) => &i.attrs,
            syn::Item::ForeignMod(i) => &i.attrs,
            syn::Item::Impl(i) => &i.attrs,
            syn::Item::Macro(i) => &i.attrs,
            syn::Item::Mod(i) => &i.attrs,
            syn::Item::Static(i) => &i.attrs,
            syn::Item::Struct(i) => &i.attrs,
            syn::Item::Trait(i) => &i.attrs,
            syn::Item::TraitAlias(i) => &i.attrs,
            syn::Item::Type(i) => &i.attrs,
            syn::Item::Union(i) => &i.attrs,
            syn::Item::Use(i) => &i.attrs,
            _ => return visit::visit_item(self, item),
        };
        if is_test(attrs) {
            return;
        }
        if let syn::Item::Mod(module) = item
            && module.content.is_none()
        {
            self.children.push(module.ident.to_string());
        }
        visit::visit_item(self, item);
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(i) => &i.attrs,
            syn::ImplItem::Fn(i) => &i.attrs,
            syn::ImplItem::Type(i) => &i.attrs,
            syn::ImplItem::Macro(i) => &i.attrs,
            _ => return visit::visit_impl_item(self, item),
        };
        if !is_test(attrs) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        match &item.tree {
            syn::UseTree::Path(path)
                if path.ident == "crate" || (self.top && path.ident == "super") =>
            {
                self.use_names(&path.tree);
            }
            _ => {}
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string());
        if let Some(first) = segments.next() {
            self.root_name(&first, segments.next());
        }
        visit::visit_path(self, path);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().is_ident("doc") || attr.path().is_ident("cfg") {
            return;
        }
        if let syn::Meta::List(list) = &attr.meta {
            self.tokens(list.tokens.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{edges, mutual};

    fn write(root: &Path, path: &str, text: &str) {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("a fixture file has a parent"))
            .expect("the fixture directory should be created");
        fs::write(file, text).expect("the fixture should be written");
    }

    #[test]
    fn a_pair_is_mutual_and_tests_macros_and_groups_are_read_as_they_build() {
        let root = std::env::temp_dir().join(format!("module-cycles-{}", std::process::id()));
        let src = root.join("src");
        write(
            &src,
            "lib.rs",
            "pub mod a;\npub mod b;\npub mod c;\npub mod d;\npub mod e;\n#[cfg(test)]\nmod t;\n",
        );
        // a and b import each other, b through a grouped use.
        write(&src, "a.rs", "use crate::b::B;\npub struct A;\n");
        write(&src, "b/mod.rs", "mod inner;\npub struct B;\n");
        write(&src, "b/inner.rs", "use crate::{a::A, d};\n");
        // c names d only inside a test module and a cfg(all(test, ..)) item.
        write(
            &src,
            "c.rs",
            "#[cfg(all(test, feature = \"x\"))]\nfn f() { crate::d::g(); }\n#[cfg(test)]\nmod tests;\n",
        );
        write(&src, "c/tests.rs", "use crate::d;\n");
        // d names e in a macro's tokens and in an attribute's, never in its
        // doc text; e names d through super:: in a top file.
        write(
            &src,
            "d.rs",
            "#[doc = \"crate::c::C\"]\n#[allow(dead_code, reason = \"x\")]\npub fn g() { tracing::warn!(error = crate::e::h(), \"x\"); }\n",
        );
        write(&src, "e.rs", "pub fn h() { super::d::g(); }\n");

        let found = edges(&src).expect("the fixture crate should scan");
        fs::remove_dir_all(&root).expect("the fixture crate should be removed");
        let pairs = mutual(&found);
        assert_eq!(
            pairs,
            [
                ("a".to_owned(), "b".to_owned()),
                ("d".to_owned(), "e".to_owned())
            ]
        );
        assert!(
            !found.contains(&("c".to_owned(), "d".to_owned())),
            "{found:?}"
        );
        assert!(
            !found.contains(&("d".to_owned(), "c".to_owned())),
            "{found:?}"
        );
        assert!(
            found.contains(&("b".to_owned(), "d".to_owned())),
            "{found:?}"
        );
    }
}
