use clap::{App, Arg};
use regex::Regex;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let matches = App::new("cargo-update-dep")
        .version("1.0")
        .author("David W. <davidwg@fb.com>")
        .about("update a Rust dependency easily")
        .arg(
            Arg::with_name("version")
                .help("the current version")
                .required(true)
                .short("v")
                .long("version")
                .takes_value(true)
                .value_name("VERSION"),
        )
        .arg(
            Arg::with_name("new_version")
                .help("the wished version")
                .required(true)
                .short("n")
                .long("new-version")
                .takes_value(true)
                .value_name("NEW_VERSION"),
        )
        .arg(
            Arg::with_name("dependency_name")
                .help("the name of the dependency")
                .required(true)
                .short("p")
                .long("dependency-name")
                .takes_value(true)
                .value_name("PACKAGE"),
        )
        .arg(
            Arg::with_name("manifest_path")
                .help("path of the main Cargo.toml to analyze (can be a workspace file)")
                .short("m")
                .long("manifest-path")
                .takes_value(true)
                .value_name("MANIFEST_PATH"),
        )
        .arg(Arg::with_name("catch-cargo-cli-bug"))
        .get_matches();

    // extract arguments
    let version = matches
        .value_of("version")
        .expect("Failed to obtain version");

    let new_version = matches
        .value_of("new_version")
        .expect("Failed to obtain new version");

    let package = matches
        .value_of("dependency_name")
        .expect("Failed to obtain dependency name");

    let root_dir = matches
        .value_of("manifest_path")
        .map(|s| {
            let mut path = PathBuf::from(s);
            path.pop(); // remove Cargo.toml
            path
        })
        .unwrap_or_else(|| std::env::current_dir().expect("Failed to open current dir"));

    run(&root_dir, package, version, new_version);
}

fn run(root_dir: &Path, package: &str, version: &str, new_version: &str) {
    // 1. fetch all Cargo.toml file via `cargo metadata | jq '.workspace_members'`
    let manifest_files = get_manifest_files(root_dir);

    // 2. update them, potentially + keep track of which ones were updated
    let mut updated = vec![];
    for manifest_file in manifest_files {
        if update_manifest_path(Path::new(&manifest_file), package, version, new_version) {
            updated.push(manifest_file);
        }
    }

    // 3. update Cargo.lock with `cargo update`
    update_cargo_lock(root_dir, package, version);

    // 4. print out files changed
    let output = Output {
        updated_manifests: updated,
    };
    let updated =
        serde_json::to_string(&output).expect("Failed to serialize updated files to string");
    println!("{}", updated);
}

#[derive(serde::Serialize)]
struct Output {
    updated_manifests: Vec<PathBuf>,
}

#[derive(serde::Deserialize)]
struct CargoMetadata {
    workspace_members: Vec<String>,
}

fn get_manifest_files(root_dir: &Path) -> Vec<PathBuf> {
    // run `cargo metadata`
    let output = Command::new("cargo")
        .current_dir(root_dir)
        .arg("metadata")
        .output()
        .expect("failed to execute process");
    assert!(output.status.success());

    // json load the result
    let cargo_metadata: CargoMetadata = serde_json::from_slice(&output.stdout)
        .expect("Failed to deserialize cargo metadata output");

    // return data
    let re = Regex::new(r"file://(.*)\)").unwrap();

    cargo_metadata
        .workspace_members
        .iter()
        .map(|path| {
            let caps = re.captures(path).expect("Failed to capture path");
            let mut path = PathBuf::from(caps.get(1).unwrap().as_str());
            path.push("Cargo.toml");
            path
        })
        .collect()
}

fn update_manifest_path(
    manifest_path: &Path,
    package: &str,
    version: &str,
    new_version: &str,
) -> bool {
    // initialize regexes (not efficient, we re-initiliaze every time...)
    let re = Regex::new(&format!(r#"^[\t\s]*{}[\t\s]*="#, package)).unwrap();
    let re2 = Regex::new(&format!(r#"package[\t\s]*=[\t\s]*"{}""#, package)).unwrap();
    let version = format!(r#""{}""#, version);
    let new_version = format!(r#""{}""#, new_version);

    // read manifest file line by line
    let mut updated = false;
    let file = File::open(manifest_path).expect("Failed to open manifest file");
    let mut lines = vec![];
    for line in BufReader::new(file).lines() {
        let mut line = line.expect("Failed to read line of file");

        // found the package
        if re.is_match(&line) || re2.is_match(&line) {
            let line2 = line.replace(&version, &new_version);
            if line != line2 {
                line = line2;
                updated = true;
            }
        }

        //
        lines.push(line);
    }

    // if the file needs change, update it
    if updated {
        let mut file = File::create(manifest_path).expect("Failed to update manifest file");
        file.write_all(lines.join("\n").as_bytes())
            .expect("Failed to write to file");
        file.write_all(b"\n").expect("Failed to write to file");
    }

    //
    updated
}

fn update_cargo_lock(root_dir: &Path, package: &str, version: &str) {
    let pkgid = format!("{}:{}", package, version);
    // run `cargo metadata`
    let _output = Command::new("cargo")
        .current_dir(root_dir)
        .args(&["update", "-p"])
        .arg(pkgid)
        .output()
        .expect("failed to execute process");
    //    assert!(output.status.success());
    // this last command might fail if the user is running something in parallel to update the Cargo.lock
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_everything() {
        // first copy our Cargo.toml so we don't rewrite it
        let mut src = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        src.push("resources/test");
        let dst = tempfile::tempdir().unwrap().into_path();
        fs::copy(
            src.as_path().join("Cargo.toml"),
            dst.as_path().join("Cargo.toml"),
        )
        .unwrap();
        fs::create_dir(dst.as_path().join("src")).unwrap();
        fs::File::create(dst.as_path().join("src/lib.rs")).unwrap();

        // run on that Cargo.toml
        run(&dst, "serde", "1.0.122", "1.0.123");
        run(&dst, "serde_json", "1.0.60", "1.0.61");
        run(&dst, "regex", "0.1.77", "1.4.3");
        run(&dst, "lazy_static", "0.2.11", "1.4.0");

        // check that it worked
        let result = fs::read_to_string(dst.as_path().join("Cargo.toml")).unwrap();
        let expected = fs::read_to_string(src.as_path().join("Cargo.toml.new")).unwrap();

        assert!(result == expected);
    }
}
