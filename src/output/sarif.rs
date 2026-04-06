use crate::types::{Category, Confidence, ScanManifest};
use serde_json::json;

const SARIF_SCHEMA: &str =
    "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json";

/// Format a ScanManifest as a SARIF v2.1.0 JSON document.
pub fn format_sarif(manifest: &ScanManifest) -> Result<String, serde_json::Error> {
    let rules: Vec<serde_json::Value> = Category::all()
        .iter()
        .map(|cat| {
            json!({
                "id": cat.to_string(),
                "name": cat.label(),
                "shortDescription": {
                    "text": format!("{} shadow dependency", cat.label())
                }
            })
        })
        .collect();

    let results: Vec<serde_json::Value> = manifest
        .deps
        .iter()
        .flat_map(|dep| {
            dep.locations.iter().map(move |loc| {
                json!({
                    "ruleId": dep.category.to_string(),
                    "level": sarif_level(dep.confidence),
                    "message": {
                        "text": format!("{}: {}", dep.category.label(), dep.name)
                    },
                    "locations": [
                        {
                            "physicalLocation": {
                                "artifactLocation": {
                                    "uri": loc.file
                                },
                                "region": {
                                    "startLine": loc.line,
                                    "startColumn": loc.col
                                }
                            }
                        }
                    ],
                    "properties": {
                        "confidence": dep.confidence.to_string(),
                        "shadowDepId": dep.id,
                        "metadata": dep.metadata.entries
                    }
                })
            })
        })
        .collect();

    let sarif = json!({
        "$schema": SARIF_SCHEMA,
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "looper",
                        "version": &manifest.tool_version,
                        "informationUri": "https://github.com/Danite/looper",
                        "rules": rules
                    }
                },
                "results": results
            }
        ]
    });

    serde_json::to_string_pretty(&sarif)
}

fn sarif_level(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::High => "error",
        Confidence::Medium => "warning",
        Confidence::Low => "note",
    }
}
