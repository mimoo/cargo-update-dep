use clap::{App, Arg};
use regex::Regex;
use std::fs::File;
use std::io::{prelude::*, BufReader, LineWriter};
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

    // 1. fetch all Cargo.toml file via `cargo metadata | jq '.workspace_members'`
    let manifest_files = get_manifest_files(&root_dir);
    println!("manifest_files: {:?}", manifest_files);

    // 2. update them, potentially + keep track of which ones were updated
    let mut updated = vec![];
    for manifest_file in manifest_files {
        if update_manifest_path(Path::new(&manifest_file), package, version, new_version) {
            println!("{:?} updated", manifest_file);
            updated.push(manifest_file);
        }
    }

    // 3. update Cargo.lock with `cargo update`
    update_cargo_lock(&root_dir, package, version, new_version);

    // 4...
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

fn update_cargo_lock(root_dir: &Path, package: &str, version: &str, new_version: &str) {
    let pkgid = format!("{}:{}", package, version);
    // run `cargo metadata`
    let output = Command::new("cargo")
        .current_dir(root_dir)
        .args(&["update", "-p"])
        .arg(pkgid)
        .arg("--precise")
        .arg(new_version)
        .output()
        .expect("failed to execute process");
    println!("{:?}", String::from_utf8(output.stdout));
    println!("{:?}", String::from_utf8(output.stderr));
    //    assert!(output.status.success());
    // this last command might fail if the user is running something in parallel to update the Cargo.lock
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regex() {
        let package = "thing";
        let version = "0.1.1";
        let new_version = "3.4.5";

        // find `PACKAGE =` or `package = "PACKAGE"`
        let re = Regex::new(r"(?P<y>\d{4})-(?P<m>\d{2})-(?P<d>\d{2})").unwrap();

        // PACKAGE = "VERSION"
        let re1 = format!(r#"{}[\t\s]*=[\t\s]*"({})""#, package, version);

        // PACKAGE = { version = "VERSION" }
        let re1_variant = format!(
            r#"{}[\t\s]*=.*version[\t\s]*=[\t\s]*"({})""#,
            package, version
        );

        // a = { package = "PACKAGE", version = "VERSION"}
        let re2 = format!(
            r#"package[\t\s]*=[\t\s]*"{}".*version[\t\s]*=[\t\s]*"({})""#,
            package, version
        );

        // a = { version = "VERSION", package = "PACKAGE"}
        let re2_variant = format!(
            r#"version[\t\s]*=[\t\s]*"({})".*package[\t\s]*=[\t\s]*"{}"#,
            version, package
        );

        let after = Regex::new(&re1)
            .unwrap()
            .replace(r#"thing = "0.1.1" "#, |caps: &regex::Captures| {
                format!("{} {}", &caps[0], &caps[0])
            });
        println!("{}", after);
    }
}
