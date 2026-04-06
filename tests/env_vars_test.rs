use looper::detectors::{default_registry, DetectorContext};
use looper::resolver::{resolve_bindings, BindingMap};
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_env_vars(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = BindingMap::new();
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    let registry = default_registry();
    registry
        .run_all(&ctx)
        .into_iter()
        .filter(|d| d.category == Category::EnvVar)
        .collect()
}

fn detect_env_vars_with_bindings(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    let registry = default_registry();
    registry
        .run_all(&ctx)
        .into_iter()
        .filter(|d| d.category == Category::EnvVar)
        .collect()
}

fn names(deps: &[ShadowDep]) -> Vec<&str> {
    deps.iter().map(|d| d.name.as_str()).collect()
}

// --- process.env.VAR_NAME (direct access) ---

#[test]
fn process_env_direct_access() {
    let deps = detect_env_vars("const url = process.env.DATABASE_URL;");
    assert_eq!(names(&deps), vec!["DATABASE_URL"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("direct_access"));
}

#[test]
fn process_env_multiple_vars() {
    let deps = detect_env_vars(
        r#"
        const a = process.env.FOO;
        const b = process.env.BAR;
        const c = process.env.BAZ;
    "#,
    );
    assert_eq!(names(&deps), vec!["FOO", "BAR", "BAZ"]);
}

// --- process.env["VAR_NAME"] (bracket access) ---

#[test]
fn process_env_bracket_access() {
    let deps = detect_env_vars(r#"const key = process.env["STRIPE_KEY"];"#);
    assert_eq!(names(&deps), vec!["STRIPE_KEY"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("bracket_access"));
}

// --- process.env[dynamic] (dynamic access) ---

#[test]
fn process_env_dynamic_access() {
    let deps = detect_env_vars("const val = process.env[someVariable];");
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].name, "<dynamic>");
    assert_eq!(deps[0].confidence, Confidence::Low);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("dynamic_access"));
}

// --- const { VAR } = process.env (destructuring) ---

#[test]
fn process_env_destructuring() {
    let deps = detect_env_vars("const { NODE_ENV, PORT, API_KEY } = process.env;");
    assert_eq!(names(&deps), vec!["NODE_ENV", "PORT", "API_KEY"]);
    for dep in &deps {
        assert_eq!(dep.confidence, Confidence::High);
        assert_eq!(dep.metadata.get_str("source_pattern"), Some("destructuring"));
    }
}

// --- process.env?.VAR (optional chaining) ---

#[test]
fn process_env_optional_chaining() {
    let deps = detect_env_vars("const url = process.env?.DATABASE_URL;");
    assert_eq!(names(&deps), vec!["DATABASE_URL"]);
    assert_eq!(deps[0].confidence, Confidence::High);
}

// --- import.meta.env.VITE_X (Vite/bundler pattern) ---

#[test]
fn import_meta_env() {
    let deps = detect_env_vars("const url = import.meta.env.VITE_API_URL;");
    assert_eq!(names(&deps), vec!["VITE_API_URL"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("import_meta_env"));
}

#[test]
fn import_meta_env_multiple() {
    let deps = detect_env_vars(
        r#"
        const a = import.meta.env.VITE_API_URL;
        const b = import.meta.env.VITE_APP_TITLE;
    "#,
    );
    assert_eq!(names(&deps), vec!["VITE_API_URL", "VITE_APP_TITLE"]);
}

// --- Deno.env.get("VAR") ---

#[test]
fn deno_env_get() {
    let deps = detect_env_vars(r#"const db = Deno.env.get("DATABASE_URL");"#);
    assert_eq!(names(&deps), vec!["DATABASE_URL"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("deno_env"));
}

// --- Bun.env.VAR ---

#[test]
fn bun_env() {
    let deps = detect_env_vars("const key = Bun.env.API_KEY;");
    assert_eq!(names(&deps), vec!["API_KEY"]);
    assert_eq!(deps[0].confidence, Confidence::High);
    assert_eq!(deps[0].metadata.get_str("source_pattern"), Some("bun_env"));
}

// --- Default/fallback tracking ---

#[test]
fn fallback_with_or() {
    let deps = detect_env_vars("const port = process.env.PORT || 3000;");
    assert_eq!(names(&deps), vec!["PORT"]);
    assert_eq!(deps[0].metadata.get_bool("required"), Some(false));
    assert_eq!(deps[0].metadata.get_str("default_value"), Some("3000"));
}

#[test]
fn fallback_with_nullish_coalescing() {
    let deps = detect_env_vars(r#"const key = process.env.API_KEY ?? "sk-default";"#);
    assert_eq!(names(&deps), vec!["API_KEY"]);
    assert_eq!(deps[0].metadata.get_bool("required"), Some(false));
    assert_eq!(deps[0].metadata.get_str("default_value"), Some("sk-default"));
}

#[test]
fn no_fallback_sets_required_true() {
    let deps = detect_env_vars("const url = process.env.DATABASE_URL;");
    assert_eq!(deps[0].metadata.get_bool("required"), Some(true));
}

#[test]
fn fallback_with_boolean_default() {
    let deps = detect_env_vars("const debug = process.env.DEBUG || false;");
    assert_eq!(deps[0].metadata.get_bool("required"), Some(false));
    assert_eq!(deps[0].metadata.get_str("default_value"), Some("false"));
}

// --- Location tracking ---

#[test]
fn location_is_correct() {
    let deps = detect_env_vars("const x = process.env.FOO;");
    assert_eq!(deps[0].locations[0].file, "test.ts");
    assert!(deps[0].locations[0].line >= 1);
    assert!(deps[0].locations[0].col >= 1);
}

// --- ID generation ---

#[test]
fn id_format() {
    let deps = detect_env_vars("const x = process.env.MY_VAR;");
    assert_eq!(deps[0].id, "env_var::MY_VAR");
}

// --- Mixed patterns ---

#[test]
fn mixed_patterns_in_one_file() {
    let deps = detect_env_vars(
        r#"
        const a = process.env.DIRECT;
        const b = process.env["BRACKET"];
        const { DESTRUCTURED } = process.env;
        const c = import.meta.env.VITE_VAR;
        const d = process.env.PORT || 8080;
    "#,
    );
    let n = names(&deps);
    assert!(n.contains(&"DIRECT"));
    assert!(n.contains(&"BRACKET"));
    assert!(n.contains(&"DESTRUCTURED"));
    assert!(n.contains(&"VITE_VAR"));
    assert!(n.contains(&"PORT"));
}

// --- Edge cases ---

#[test]
fn empty_source_no_findings() {
    let deps = detect_env_vars("");
    assert!(deps.is_empty());
}

#[test]
fn no_env_access_no_findings() {
    let deps = detect_env_vars("const x = 42; function foo() { return x; }");
    assert!(deps.is_empty());
}

#[test]
fn process_not_env_no_findings() {
    let deps = detect_env_vars("const pid = process.pid;");
    assert!(deps.is_empty());
}

#[test]
fn nested_in_function() {
    let deps = detect_env_vars(
        r#"
        function getConfig() {
            return process.env.CONFIG_PATH;
        }
    "#,
    );
    assert_eq!(names(&deps), vec!["CONFIG_PATH"]);
}

#[test]
fn nested_in_arrow_function() {
    let deps = detect_env_vars("const getPort = () => process.env.PORT || 3000;");
    assert_eq!(names(&deps), vec!["PORT"]);
    assert_eq!(deps[0].metadata.get_bool("required"), Some(false));
}

// --- dotenv usage flagging ---

#[test]
fn dotenv_require_config() {
    let deps = detect_env_vars(r#"require("dotenv").config();"#);
    assert_eq!(names(&deps), vec!["<dotenv>"]);
    assert_eq!(deps[0].confidence, Confidence::Medium);
    assert_eq!(
        deps[0].metadata.get_str("source_pattern"),
        Some("dotenv_config")
    );
}

#[test]
fn dotenv_import_side_effect() {
    let deps = detect_env_vars(r#"import "dotenv/config";"#);
    assert_eq!(names(&deps), vec!["<dotenv>"]);
    assert_eq!(deps[0].confidence, Confidence::Medium);
}

#[test]
fn dotenv_import_bare() {
    let deps = detect_env_vars(r#"import "dotenv";"#);
    assert_eq!(names(&deps), vec!["<dotenv>"]);
}

#[test]
fn dotenv_esm_explicit() {
    let deps = detect_env_vars_with_bindings(
        r#"import dotenv from "dotenv";
dotenv.config();"#,
    );
    assert!(names(&deps).contains(&"<dotenv>"));
}

#[test]
fn dotenv_require_variable() {
    let deps = detect_env_vars_with_bindings(
        r#"const dotenv = require("dotenv");
dotenv.config();"#,
    );
    assert!(names(&deps).contains(&"<dotenv>"));
}

#[test]
fn dotenv_not_triggered_by_random_config() {
    let deps = detect_env_vars("someLib.config();");
    assert!(deps.is_empty());
}

// --- envalid schema extraction ---

#[test]
fn envalid_cleanenv_extracts_keys() {
    let deps = detect_env_vars_with_bindings(
        r#"
        import envalid from "envalid";
        envalid.cleanEnv(process.env, {
            DATABASE_URL: envalid.str(),
            API_KEY: envalid.str(),
        });
        "#,
    );
    let env_names = names(&deps);
    assert!(env_names.contains(&"DATABASE_URL"));
    assert!(env_names.contains(&"API_KEY"));
}

#[test]
fn envalid_cleanenv_with_defaults() {
    let deps = detect_env_vars_with_bindings(
        r#"
        import envalid from "envalid";
        envalid.cleanEnv(process.env, {
            PORT: envalid.port({ default: 3000 }),
            HOST: envalid.str({ default: "localhost" }),
        });
        "#,
    );
    let port = deps.iter().find(|d| d.name == "PORT").unwrap();
    assert_eq!(port.metadata.get_bool("required"), Some(false));
    assert_eq!(port.metadata.get_str("default_value"), Some("3000"));

    let host = deps.iter().find(|d| d.name == "HOST").unwrap();
    assert_eq!(host.metadata.get_bool("required"), Some(false));
    assert_eq!(host.metadata.get_str("default_value"), Some("localhost"));
}

#[test]
fn envalid_required_without_default() {
    let deps = detect_env_vars_with_bindings(
        r#"
        import envalid from "envalid";
        envalid.cleanEnv(process.env, {
            SECRET: envalid.str(),
        });
        "#,
    );
    let secret = deps.iter().find(|d| d.name == "SECRET").unwrap();
    assert_eq!(secret.metadata.get_bool("required"), Some(true));
    assert_eq!(secret.metadata.get_str("source_pattern"), Some("envalid"));
    assert_eq!(secret.confidence, Confidence::High);
}

#[test]
fn envalid_require_cjs() {
    let deps = detect_env_vars_with_bindings(
        r#"
        const envalid = require("envalid");
        envalid.cleanEnv(process.env, {
            DB_HOST: envalid.host(),
        });
        "#,
    );
    assert!(names(&deps).contains(&"DB_HOST"));
}

#[test]
fn envalid_not_triggered_by_random_cleanenv() {
    let deps = detect_env_vars("someLib.cleanEnv(process.env, { X: 1 });");
    let envalid_deps: Vec<_> = deps
        .iter()
        .filter(|d| d.metadata.get_str("source_pattern") == Some("envalid"))
        .collect();
    assert!(envalid_deps.is_empty());
}
