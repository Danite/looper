use looper::resolver::{resolve_bindings, ModuleBinding};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn bindings_from(source: &str) -> std::collections::HashMap<String, ModuleBinding> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    resolve_bindings(&ret.program)
}

fn bindings_from_cjs(source: &str) -> std::collections::HashMap<String, ModuleBinding> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::cjs()).parse();
    resolve_bindings(&ret.program)
}

#[test]
fn esm_named_import() {
    let map = bindings_from("import { readFileSync, writeFile } from 'fs';");

    assert_eq!(
        map.get("readFileSync"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "readFileSync".into(),
        })
    );
    assert_eq!(
        map.get("writeFile"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "writeFile".into(),
        })
    );
}

#[test]
fn esm_named_import_with_alias() {
    let map = bindings_from("import { exec as execute } from 'child_process';");

    assert_eq!(
        map.get("execute"),
        Some(&ModuleBinding {
            module_source: "child_process".into(),
            imported_name: "exec".into(),
        })
    );
    assert!(!map.contains_key("exec"));
}

#[test]
fn esm_default_import() {
    let map = bindings_from("import fs from 'fs';");

    assert_eq!(
        map.get("fs"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "*".into(),
        })
    );
}

#[test]
fn esm_namespace_import() {
    let map = bindings_from("import * as path from 'path';");

    assert_eq!(
        map.get("path"),
        Some(&ModuleBinding {
            module_source: "path".into(),
            imported_name: "*".into(),
        })
    );
}

#[test]
fn cjs_require_default() {
    let map = bindings_from_cjs("const fs = require('fs');");

    assert_eq!(
        map.get("fs"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "*".into(),
        })
    );
}

#[test]
fn cjs_require_destructured() {
    let map = bindings_from_cjs("const { readFile, writeFile } = require('fs');");

    assert_eq!(
        map.get("readFile"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "readFile".into(),
        })
    );
    assert_eq!(
        map.get("writeFile"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "writeFile".into(),
        })
    );
}

#[test]
fn node_prefix_normalization() {
    let map = bindings_from("import fs from 'node:fs';");

    assert_eq!(
        map.get("fs"),
        Some(&ModuleBinding {
            module_source: "fs".into(),
            imported_name: "*".into(),
        })
    );
}

#[test]
fn node_prefix_normalization_cjs() {
    let map = bindings_from_cjs("const cp = require('node:child_process');");

    assert_eq!(
        map.get("cp"),
        Some(&ModuleBinding {
            module_source: "child_process".into(),
            imported_name: "*".into(),
        })
    );
}

#[test]
fn node_prefix_named_import() {
    let map = bindings_from("import { spawn } from 'node:child_process';");

    assert_eq!(
        map.get("spawn"),
        Some(&ModuleBinding {
            module_source: "child_process".into(),
            imported_name: "spawn".into(),
        })
    );
}

#[test]
fn subpath_imports() {
    let map = bindings_from("import { readFile } from 'fs/promises';");

    assert_eq!(
        map.get("readFile"),
        Some(&ModuleBinding {
            module_source: "fs/promises".into(),
            imported_name: "readFile".into(),
        })
    );
}

#[test]
fn multiple_imports() {
    let map = bindings_from(
        r#"
        import fs from 'fs';
        import { exec } from 'child_process';
        import * as http from 'http';
    "#,
    );

    assert_eq!(map.len(), 3);
    assert_eq!(map["fs"].module_source, "fs");
    assert_eq!(map["exec"].module_source, "child_process");
    assert_eq!(map["http"].module_source, "http");
}

#[test]
fn ignores_non_require_calls() {
    let map = bindings_from_cjs("const result = someFunction('fs');");
    assert!(map.is_empty());
}

#[test]
fn ignores_dynamic_require() {
    let map = bindings_from_cjs("const mod = require(dynamicVar);");
    assert!(map.is_empty());
}

#[test]
fn empty_program() {
    let map = bindings_from("");
    assert!(map.is_empty());
}

#[test]
fn third_party_modules() {
    let map = bindings_from("import axios from 'axios';");

    assert_eq!(
        map.get("axios"),
        Some(&ModuleBinding {
            module_source: "axios".into(),
            imported_name: "*".into(),
        })
    );
}
