use looper::detectors::ports::PortsDetector;
use looper::detectors::{DetectorContext, DetectorRegistry};
use looper::resolver::resolve_bindings;
use looper::types::{Category, Confidence, ShadowDep};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

fn detect_ports(source: &str) -> Vec<ShadowDep> {
    let allocator = Allocator::new();
    let ret = Parser::new(&allocator, source, SourceType::mjs()).parse();
    let program = allocator.alloc(ret.program);
    let bindings = resolve_bindings(program);
    let mut registry = DetectorRegistry::new();
    registry.register(Box::new(PortsDetector));
    let ctx = DetectorContext {
        program,
        source_text: source,
        file_path: "test.ts",
        bindings: &bindings,
    };
    registry.run_all(&ctx)
}

fn port_deps(deps: &[ShadowDep]) -> Vec<&ShadowDep> {
    deps.iter().filter(|d| d.category == Category::Port).collect()
}

fn net_deps(deps: &[ShadowDep]) -> Vec<&ShadowDep> {
    deps.iter().filter(|d| d.category == Category::Network).collect()
}

// --- app.listen / server.listen ---

#[test]
fn app_listen_numeric() {
    let deps = detect_ports("app.listen(3000);");
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":3000");
    assert_eq!(ports[0].metadata.get_str("direction"), Some("listen"));
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("http"));
}

#[test]
fn server_listen_numeric() {
    let deps = detect_ports("server.listen(8080);");
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":8080");
}

#[test]
fn listen_with_env_fallback() {
    let deps = detect_ports("server.listen(process.env.PORT || 8080);");
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":8080");
    assert_eq!(ports[0].metadata.get_str("direction"), Some("listen"));
}

#[test]
fn listen_id_format() {
    let deps = detect_ports("app.listen(3000);");
    let ports = port_deps(&deps);
    assert_eq!(ports[0].id, "port::3000::listen");
}

// --- net.createConnection ---

#[test]
fn net_create_connection() {
    let deps = detect_ports(r#"
        import net from 'net';
        net.createConnection({ port: 6379 });
    "#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":6379");
    assert_eq!(ports[0].metadata.get_str("direction"), Some("connect"));
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("tcp"));
}

#[test]
fn net_connect() {
    let deps = detect_ports(r#"
        import net from 'node:net';
        net.connect({ port: 5432 });
    "#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":5432");
}

#[test]
fn net_without_binding_ignored() {
    let deps = detect_ports(r#"
        const net = { createConnection: () => {} };
        net.createConnection({ port: 9999 });
    "#);
    assert!(port_deps(&deps).is_empty());
}

// --- Connection strings ---

#[test]
fn redis_connection_string() {
    let deps = detect_ports(r#"new Redis("redis://localhost:6379");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":6379");
    assert_eq!(ports[0].metadata.get_str("direction"), Some("connect"));
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("redis"));

    let nets = net_deps(&deps);
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0].name, "localhost");
}

#[test]
fn mongodb_connection_string() {
    let deps = detect_ports(r#"mongoose.connect("mongodb://localhost:27017/mydb");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":27017");
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("mongodb"));

    let nets = net_deps(&deps);
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0].name, "localhost");
}

#[test]
fn postgres_connection_string() {
    let deps = detect_ports(r#"pool.connect("postgres://user@db.internal:5432/mydb");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":5432");
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("postgres"));

    let nets = net_deps(&deps);
    assert_eq!(nets.len(), 1);
    assert_eq!(nets[0].name, "db.internal");
}

#[test]
fn mysql_connection_string() {
    let deps = detect_ports(r#"mysql.createConnection("mysql://root@localhost:3306/db");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":3306");
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("mysql"));
}

#[test]
fn connection_string_default_port() {
    let deps = detect_ports(r#"new Redis("redis://cache.internal");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":6379");
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("redis"));
}

#[test]
fn amqp_connection_string() {
    let deps = detect_ports(r#"amqp.connect("amqp://rabbitmq.local:5672");"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":5672");
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("amqp"));
}

// --- Bun.serve ---

#[test]
fn bun_serve() {
    let deps = detect_ports(r#"Bun.serve({ port: 3000 });"#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, ":3000");
    assert_eq!(ports[0].metadata.get_str("direction"), Some("listen"));
    assert_eq!(ports[0].metadata.get_str("protocol_hint"), Some("http"));
}

// --- Edge cases ---

#[test]
fn empty_source() {
    let deps = detect_ports("");
    assert!(deps.is_empty());
}

#[test]
fn unknown_protocol_ignored() {
    let deps = detect_ports(r#"client.connect("ftp://files.example.com:21");"#);
    assert!(port_deps(&deps).is_empty());
}

#[test]
fn http_url_not_matched_as_connection_string() {
    // http:// URLs are handled by the network detector, not ports
    let deps = detect_ports(r#"fetch("http://api.example.com:8080/data");"#);
    assert!(port_deps(&deps).is_empty());
}

#[test]
fn all_findings_high_confidence() {
    let deps = detect_ports(r#"
        app.listen(3000);
        new Redis("redis://localhost:6379");
    "#);
    for dep in &deps {
        assert_eq!(dep.confidence, Confidence::High);
    }
}

#[test]
fn multiple_ports() {
    let deps = detect_ports(r#"
        app.listen(3000);
        new Redis("redis://localhost:6379");
        mongoose.connect("mongodb://db:27017/app");
    "#);
    let ports = port_deps(&deps);
    assert_eq!(ports.len(), 3);
    let names: Vec<&str> = ports.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&":3000"));
    assert!(names.contains(&":6379"));
    assert!(names.contains(&":27017"));
}
