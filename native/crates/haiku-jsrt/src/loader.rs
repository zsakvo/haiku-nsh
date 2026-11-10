use crate::JsError;
use rquickjs::{
    Ctx, Module,
    loader::{ImportAttributes, Loader, Resolver},
    module::Declared,
};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

/// A native module factory, invoked only on the instance's VM thread.
pub type NativeModule = for<'js> fn(&Ctx<'js>, &str) -> rquickjs::Result<Module<'js, Declared>>;

#[derive(Default)]
pub(crate) struct Sources {
    pub root: Option<PathBuf>,
    pub maps: BTreeMap<String, oxc_sourcemap::SourceMap<'static>>,
}
pub(crate) type SharedSources = Arc<Mutex<Sources>>;

pub(crate) struct PackageResolver {
    pub sources: SharedSources,
    pub names: Vec<String>,
}

pub(crate) fn checked_path(root: &Path, base: &Path, name: &str) -> Result<PathBuf, JsError> {
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(JsError::new(
            "ModuleError",
            "Absolute paths and parent traversal are forbidden",
        ));
    }
    let path = base
        .join(path)
        .canonicalize()
        .map_err(|e| JsError::new("ModuleError", e.to_string()))?;
    if !path.starts_with(root) {
        return Err(JsError::new(
            "ModuleError",
            "Module escapes package directory",
        ));
    }
    Ok(path)
}
impl Resolver for PackageResolver {
    fn resolve<'js>(
        &mut self,
        _: &Ctx<'js>,
        base: &str,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if self.names.iter().any(|n| n == name) {
            return Ok(name.into());
        }
        let guard = self.sources.lock().unwrap();
        let root = guard
            .root
            .as_ref()
            .ok_or_else(|| rquickjs::Error::new_resolving(base, name))?;
        if !name.starts_with("./") {
            return Err(rquickjs::Error::new_resolving_message(
                base,
                name,
                "Only package-relative imports are allowed",
            ));
        }
        let base = Path::new(base).parent().unwrap_or(root);
        checked_path(root, base, name)
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| {
                rquickjs::Error::new_resolving_message(base.to_string_lossy(), name, e.message)
            })
    }
}
pub(crate) struct PackageLoader {
    pub sources: SharedSources,
    pub modules: BTreeMap<String, String>,
    pub native: BTreeMap<String, NativeModule>,
}
impl Loader for PackageLoader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js, Declared>> {
        if let Some(factory) = self.native.get(name) {
            return factory(ctx, name);
        }
        if let Some(source) = self.modules.get(name) {
            return Module::declare(ctx.clone(), name, source.as_bytes());
        }
        let mut sources = self.sources.lock().unwrap();
        let root = sources
            .root
            .as_ref()
            .ok_or_else(|| rquickjs::Error::new_loading(name))?;
        let path = Path::new(name)
            .canonicalize()
            .map_err(|e| rquickjs::Error::new_loading_message(name, e.to_string()))?;
        if !path.starts_with(root) {
            return Err(rquickjs::Error::new_loading_message(
                name,
                "Module escapes package",
            ));
        }
        let source = read_source(&path, &mut sources)
            .map_err(|e| rquickjs::Error::new_loading_message(name, e.message))?;
        // Declaring a module may recursively load its imports.
        drop(sources);
        Module::declare(ctx.clone(), name, source)
    }
}
pub(crate) fn read_source(path: &Path, sources: &mut Sources) -> Result<String, JsError> {
    let text =
        std::fs::read_to_string(path).map_err(|e| JsError::new("ModuleError", e.to_string()))?;
    if path.extension().is_none_or(|e| e != "ts") {
        return Ok(text);
    }
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, &text, oxc_span::SourceType::ts()).parse();
    if !parsed.diagnostics.is_empty() {
        return Err(JsError::new(
            "SyntaxError",
            format!("{}: {:?}", path.display(), parsed.diagnostics),
        ));
    }
    let mut program = parsed.program;
    let semantic = oxc_semantic::SemanticBuilder::new().build(&program);
    if !semantic.diagnostics.is_empty() {
        return Err(JsError::new(
            "SyntaxError",
            format!("{:?}", semantic.diagnostics),
        ));
    }
    let transformed = oxc_transformer::Transformer::new(
        &allocator,
        path,
        &oxc_transformer::TransformOptions::default(),
    )
    .build_with_scoping(semantic.semantic.into_scoping(), &mut program);
    if !transformed.diagnostics.is_empty() {
        return Err(JsError::new(
            "SyntaxError",
            format!("{:?}", transformed.diagnostics),
        ));
    }
    let output = oxc_codegen::Codegen::new()
        .with_options(oxc_codegen::CodegenOptions {
            source_map_path: Some(path.to_owned()),
            ..Default::default()
        })
        .build(&program);
    if let Some(map) = output.map {
        sources
            .maps
            .insert(path.to_string_lossy().into_owned(), map.into_owned());
    }
    Ok(output.code)
}

pub(crate) fn locate(error: &mut JsError, sources: &SharedSources) {
    let sources = sources.lock().unwrap();
    let mut stack = error.stack.clone();
    for frame in error.stack.lines() {
        let frame = frame.trim().trim_end_matches(')');
        let Some((prefix, col)) = frame.rsplit_once(':') else {
            continue;
        };
        let Ok(col) = col.parse::<u32>() else {
            continue;
        };
        let Some((prefix, line)) = prefix.rsplit_once(':') else {
            continue;
        };
        let Ok(line) = line.parse::<u32>() else {
            continue;
        };
        let file = prefix
            .rsplit_once('(')
            .map_or(prefix.trim_start_matches("at "), |(_, f)| f);
        let (mapped_line, mapped_col) = sources
            .maps
            .get(file)
            .and_then(|map| {
                map.lookup_token_approx(
                    &map.generate_lookup_table(),
                    line.saturating_sub(1),
                    col.saturating_sub(1),
                )
                .map(|t| (t.get_src_line() + 1, t.get_src_col() + 1))
            })
            .unwrap_or((line, col));
        stack = stack.replace(
            &format!("{file}:{line}:{col}"),
            &format!("{file}:{mapped_line}:{mapped_col}"),
        );
        if error.file.is_none() {
            error.file = Some(file.into());
            error.line = Some(mapped_line);
            error.column = Some(mapped_col);
        }
    }
    error.stack = stack;
}
