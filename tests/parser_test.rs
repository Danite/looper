use looper::parser::parse_file;
use oxc_allocator::Allocator;
use std::path::Path;

#[test]
fn parses_valid_typescript() {
    let allocator = Allocator::new();
    let source = r#"
        const x: number = 42;
        function greet(name: string): string {
            return `Hello, ${name}`;
        }
        interface User {
            id: number;
            name: string;
        }
    "#;

    let result = parse_file(&allocator, Path::new("test.ts"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
    assert_eq!(result.file_path, "test.ts");
    assert!(!result.program.body.is_empty());
}

#[test]
fn parses_valid_javascript() {
    let allocator = Allocator::new();
    let source = r#"
        const x = 42;
        function greet(name) {
            return `Hello, ${name}`;
        }
    "#;

    let result = parse_file(&allocator, Path::new("app.js"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
}

#[test]
fn parses_valid_jsx() {
    let allocator = Allocator::new();
    let source = r#"
        function App() {
            return <div className="app"><h1>Hello</h1></div>;
        }
    "#;

    let result = parse_file(&allocator, Path::new("component.jsx"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
}

#[test]
fn parses_valid_tsx() {
    let allocator = Allocator::new();
    let source = r#"
        interface Props { name: string; }
        function App({ name }: Props) {
            return <div>{name}</div>;
        }
    "#;

    let result = parse_file(&allocator, Path::new("component.tsx"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
}

#[test]
fn parses_esm_module() {
    let allocator = Allocator::new();
    let source = r#"
        import { readFile } from 'fs';
        export const data = readFile('./data.json');
    "#;

    let result = parse_file(&allocator, Path::new("utils.mjs"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
}

#[test]
fn parses_commonjs_module() {
    let allocator = Allocator::new();
    let source = r#"
        const fs = require('fs');
        module.exports = { read: fs.readFileSync };
    "#;

    let result = parse_file(&allocator, Path::new("utils.cjs"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
}

#[test]
fn recovers_from_syntax_errors() {
    let allocator = Allocator::new();
    // Duplicate declarations and type errors that oxc can recover from
    let source = r#"
        const x = 42;
        let x = 99;
        const y: string = 123;
        import { foo } from ;
        const z = 100;
    "#;

    let result = parse_file(&allocator, Path::new("broken.ts"), source);

    // The key requirement: parsing doesn't crash, and we get diagnostics.
    // Whether the body is empty or not depends on how severe the error is.
    assert!(result.has_errors() || result.panicked);
    assert!(!result.errors.is_empty());
}

#[test]
fn handles_severely_broken_file() {
    let allocator = Allocator::new();
    let source = r#"
        function broken( {
        {{{{
    "#;

    let result = parse_file(&allocator, Path::new("severe.ts"), source);

    // Even with severe errors, the function must not crash.
    // It's OK if panicked is true and body is empty.
    assert!(result.has_errors() || result.panicked);
}

#[test]
fn preserves_source_text_reference() {
    let allocator = Allocator::new();
    let source = "const x = 1;";

    let result = parse_file(&allocator, Path::new("test.js"), source);

    assert_eq!(result.source_text, source);
}

#[test]
fn handles_empty_file() {
    let allocator = Allocator::new();
    let source = "";

    let result = parse_file(&allocator, Path::new("empty.ts"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
    assert!(result.program.body.is_empty());
}

#[test]
fn handles_process_env_pattern() {
    let allocator = Allocator::new();
    let source = r#"
        const dbUrl = process.env.DATABASE_URL;
        const port = process.env.PORT || 3000;
        const { NODE_ENV, API_KEY } = process.env;
    "#;

    let result = parse_file(&allocator, Path::new("config.ts"), source);

    assert!(!result.panicked);
    assert!(!result.has_errors());
    assert!(!result.program.body.is_empty());
}

#[test]
fn file_path_stored_correctly() {
    let allocator = Allocator::new();
    let source = "const x = 1;";

    let result = parse_file(&allocator, Path::new("src/deep/nested/file.tsx"), source);

    assert_eq!(result.file_path, "src/deep/nested/file.tsx");
}
