use std::collections::HashSet;
use std::path::Path;

/// Package name -> binary names it provides.
pub const PACKAGE_TO_BINARIES: &[(&str, &[&str])] = &[
    ("ffmpeg", &["ffmpeg", "ffprobe"]),
    ("imagemagick", &["convert", "mogrify", "identify", "composite"]),
    ("graphicsmagick", &["gm"]),
    ("git", &["git"]),
    ("curl", &["curl"]),
    ("wget", &["wget"]),
    ("jq", &["jq"]),
    ("chromium", &["chromium", "chromium-browser"]),
    ("chromium-browser", &["chromium", "chromium-browser"]),
    ("python3", &["python3", "python"]),
    ("python", &["python", "python3"]),
    ("nodejs", &["node", "nodejs"]),
    ("npm", &["npm", "npx"]),
    ("docker", &["docker"]),
    ("docker.io", &["docker"]),
    ("openssh-client", &["ssh", "scp"]),
    ("rsync", &["rsync"]),
    ("zip", &["zip"]),
    ("unzip", &["unzip"]),
    ("make", &["make"]),
    ("gcc", &["gcc"]),
    ("g++", &["g++"]),
];

/// Base image -> binary it provides.
const IMAGE_TO_BINARY: &[(&str, &str)] = &[
    ("node", "node"),
    ("python", "python"),
    ("golang", "go"),
    ("rust", "rustc"),
    ("ruby", "ruby"),
    ("php", "php"),
    ("openjdk", "java"),
    ("amazoncorretto", "java"),
    ("nginx", "nginx"),
];

#[derive(Debug, Default)]
pub struct DockerfileResult {
    pub base_image: Option<String>,
    pub binaries: HashSet<String>,
    pub exposed_ports: Vec<u16>,
    pub copy_sources: Vec<String>,
    pub source_file: Option<String>,
}

/// Parse a Dockerfile extracting FROM, RUN install, EXPOSE, COPY/ADD.
pub fn parse_dockerfile(path: &Path) -> Result<DockerfileResult, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;

    let source_file = path.to_string_lossy().into_owned();
    let mut result = DockerfileResult {
        source_file: Some(source_file),
        ..Default::default()
    };

    let logical_lines = join_continuation_lines(&content);

    for line in &logical_lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let upper = trimmed.to_uppercase();

        if upper.starts_with("FROM ") {
            handle_from(trimmed, &mut result);
        } else if upper.starts_with("RUN ") {
            handle_run(trimmed, &mut result);
        } else if upper.starts_with("EXPOSE ") {
            handle_expose(trimmed, &mut result);
        } else if upper.starts_with("COPY ") || upper.starts_with("ADD ") {
            handle_copy(trimmed, &mut result);
        }
    }

    Ok(result)
}

/// Auto-detect Dockerfile in project root.
pub fn find_and_parse_dockerfile(project_root: &Path) -> DockerfileResult {
    for filename in &["Dockerfile", "dockerfile", "Dockerfile.dev", "Dockerfile.prod"] {
        let path = project_root.join(filename);
        if path.exists() {
            if let Ok(result) = parse_dockerfile(&path) {
                return result;
            }
        }
    }
    DockerfileResult::default()
}

/// Join lines ending with `\` into single logical lines.
fn join_continuation_lines(content: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in content.lines() {
        if let Some(stripped) = line.strip_suffix('\\') {
            current.push_str(stripped);
            current.push(' ');
        } else {
            current.push_str(line);
            lines.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn handle_from(line: &str, result: &mut DockerfileResult) {
    // FROM image:tag [AS name]
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let image_full = parts[1];
        result.base_image = Some(image_full.to_string());

        let image_name = image_full.split(':').next().unwrap_or(image_full);
        // Strip registry prefix if present
        let short_name = image_name.rsplit('/').next().unwrap_or(image_name);

        for (img, binary) in IMAGE_TO_BINARY {
            if short_name == *img {
                result.binaries.insert(binary.to_string());
                break;
            }
        }
    }
}

fn handle_run(line: &str, result: &mut DockerfileResult) {
    let run_body = line[4..].trim(); // skip "RUN "

    let shell_str = if run_body.starts_with('[') {
        parse_exec_form(run_body)
    } else {
        run_body.to_string()
    };

    let commands: Vec<&str> = shell_str.split("&&").collect();
    for cmd in commands {
        let cmd = cmd.trim();
        extract_install_packages(cmd, result);
    }
}

fn parse_exec_form(s: &str) -> String {
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(s) {
        arr.join(" ")
    } else {
        let trimmed = s.trim_start_matches('[').trim_end_matches(']');
        trimmed
            .split(',')
            .map(|part| part.trim().trim_matches('"').trim_matches('\''))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn extract_install_packages(cmd: &str, result: &mut DockerfileResult) {
    let tokens: Vec<&str> = cmd.split_whitespace().collect();
    if tokens.is_empty() {
        return;
    }

    // Detect: apt-get install, apt install, apk add, yum install, dnf install, pacman -S
    let is_install = match tokens.as_slice() {
        [cmd, "install", ..] if *cmd == "apt-get" || *cmd == "apt" || *cmd == "yum" || *cmd == "dnf" => true,
        [cmd, "add", ..] if *cmd == "apk" => true,
        [cmd, flag, ..] if *cmd == "pacman" && flag.contains('S') => true,
        _ => false,
    };

    if !is_install {
        return;
    }

    let skip = 2;
    for token in &tokens[skip..] {
        if token.starts_with('-') || token.starts_with("--") {
            continue;
        }
        // Skip common non-package tokens
        if *token == "&&" || *token == "||" || *token == ";" {
            break;
        }

        let package = token.trim();
        if package.is_empty() {
            continue;
        }

        // Look up in package-to-binary mapping
        let mut matched = false;
        for (pkg, bins) in PACKAGE_TO_BINARIES {
            if package == *pkg {
                for bin in *bins {
                    result.binaries.insert(bin.to_string());
                }
                matched = true;
                break;
            }
        }

        if !matched {
            // Assume package name = binary name as fallback
            result.binaries.insert(package.to_string());
        }
    }
}

fn handle_expose(line: &str, result: &mut DockerfileResult) {
    // EXPOSE 3000 8080 443/tcp
    for token in line.split_whitespace().skip(1) {
        let port_str = token.split('/').next().unwrap_or(token);
        if let Ok(port) = port_str.parse::<u16>() {
            result.exposed_ports.push(port);
        }
    }
}

fn handle_copy(line: &str, result: &mut DockerfileResult) {
    // COPY [--from=...] [--chown=...] src... dest
    let tokens: Vec<&str> = line.split_whitespace().skip(1).collect();
    // Skip flags and take all but last (which is destination)
    let non_flag: Vec<&&str> = tokens.iter().filter(|t| !t.starts_with("--")).collect();
    if non_flag.len() >= 2 {
        for src in &non_flag[..non_flag.len() - 1] {
            result.copy_sources.push(src.to_string());
        }
    }
}
