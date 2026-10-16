use std::fs;
use std::path::Path;

use ncp_matcher::{Config, Utf32String};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Manifest {
    datasets: Vec<Source>,
}

#[derive(Deserialize)]
struct Source {
    name: String,
    outputs: Vec<Fixture>,
}

#[derive(Deserialize)]
struct Fixture {
    file: String,
    rows: usize,
    sha256: String,
}

pub struct Dataset {
    pub name: &'static str,
    pub queries: &'static [(&'static str, &'static str)],
    pub short_query: &'static str,
    pub long_query: &'static str,
}

pub const DATASETS: &[Dataset] = &[
    Dataset {
        name: "chromium",
        queries: &[
            ("broad", "src"),
            ("selective", "linux"),
            ("multi_atom", "linux test"),
            ("substring", "'linux"),
            ("prefix", "^/chromium/chrome"),
            ("no_match", "ncp927missingcandidate927"),
        ],
        short_query: "li",
        long_query: "linux",
    },
    Dataset {
        name: "wikipedia-de",
        queries: &[
            ("broad", "er"),
            ("selective", "berlin"),
            ("multi_atom", "berlin bahn"),
            ("normalization", "munchen"),
            ("substring", "'Berlin"),
            ("no_match", "ncp927missingcandidate927"),
        ],
        short_query: "ber",
        long_query: "berlin",
    },
    Dataset {
        name: "wikipedia-ja",
        queries: &[
            ("broad", "学"),
            ("selective", "東京"),
            ("multi_atom", "東京 大学"),
            ("ascii", "JR"),
            ("substring", "'東京"),
            ("no_match", "ncp927missingcandidate927"),
        ],
        short_query: "東",
        long_query: "東京",
    },
    Dataset {
        name: "wikipedia-ar",
        queries: &[
            ("broad", "ال"),
            ("selective", "القاهرة"),
            ("multi_atom", "جامعة القاهرة"),
            ("ascii", "FC"),
            ("substring", "'القاهرة"),
            ("no_match", "ncp927missingcandidate927"),
        ],
        short_query: "القا",
        long_query: "القاهرة",
    },
];

pub struct Corpus {
    pub items: Vec<Utf32String>,
    pub config: Config,
}

impl Dataset {
    pub fn load(&self, size: &str) -> Corpus {
        let manifest: Manifest = serde_json::from_str(include_str!("../data/manifest.json"))
            .expect("invalid benchmark data manifest");
        let filename = if size == "full" && self.name == "chromium" {
            "chromium-full.txt".to_owned()
        } else {
            format!("{}.txt", self.name)
        };
        let source = manifest
            .datasets
            .iter()
            .find(|source| source.name == self.name)
            .expect("dataset missing from manifest");
        let fixture = source
            .outputs
            .iter()
            .find(|fixture| fixture.file == filename)
            .expect("fixture missing from manifest");
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("benches/data/prepared")
            .join(filename);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "Cannot read {}: {error}. Run `python3 benches/prepare_data.py` first.",
                path.display()
            )
        });
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            fixture.sha256,
            "Fixture checksum mismatch: {}",
            path.display()
        );
        let text = std::str::from_utf8(&bytes).expect("fixture must be UTF-8");
        assert_eq!(text.lines().count(), fixture.rows);
        let rows = if size == "full" {
            fixture.rows
        } else {
            size.parse::<usize>().expect("invalid sample size")
        };
        assert!(rows <= fixture.rows);
        let items: Vec<Utf32String> = text.lines().take(rows).map(Utf32String::from).collect();
        Corpus {
            items,
            config: if self.name == "chromium" {
                Config::DEFAULT.match_paths()
            } else {
                Config::DEFAULT
            },
        }
    }
}
