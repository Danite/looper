use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_parser::Parser;
use oxc_span::SourceType;
use std::path::Path;

/// Result of parsing a single JS/TS file.
pub struct ParseResult<'a> {
    pub program: Program<'a>,
    pub source_text: &'a str,
    pub file_path: String,
    pub errors: Vec<String>,
    pub panicked: bool,
}

impl<'a> ParseResult<'a> {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Parse a JS/TS file into an AST.
///
/// The caller owns the `Allocator`; the returned `Program` borrows from it.
/// On parse errors, the partial AST is still returned (error recovery).
/// If the parser panics (unrecoverable), `panicked` will be true and the
/// program will be empty.
pub fn parse_file<'a>(
    allocator: &'a Allocator,
    file_path: &Path,
    source_text: &'a str,
) -> ParseResult<'a> {
    let source_type = source_type_from_path(file_path);

    let parser_return = Parser::new(allocator, source_text, source_type).parse();

    let errors: Vec<String> = parser_return
        .errors
        .iter()
        .map(|e| e.to_string())
        .collect();

    ParseResult {
        program: parser_return.program,
        source_text,
        file_path: file_path.to_string_lossy().into_owned(),
        errors,
        panicked: parser_return.panicked,
    }
}

/// Determine the oxc SourceType from a file extension.
fn source_type_from_path(path: &Path) -> SourceType {
    SourceType::from_path(path).unwrap_or_default()
}
