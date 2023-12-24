use std::fs;
use std::process::Command;

fn main() {
    let mut output = Command::new("xcrun")
        .args(&[
            "-sdk",
            "macosx",
            "metal",
            "-c",
            "shaders.metal",
            "-o",
            "shaders.air",
        ])
        .output()
        .unwrap();

    if !output.stderr.is_empty() {
        panic!(
            "Failed to create air file: {}",
            std::str::from_utf8(&output.stderr).unwrap()
        );
    }

    output = Command::new("xcrun")
        .args(&[
            "-sdk",
            "macosx",
            "metallib",
            "shaders.air",
            "-o",
            "shaders.metallib",
        ])
        .output()
        .unwrap();

    if !output.stderr.is_empty() {
        panic!(
            "Failed to create metallib: {}",
            std::str::from_utf8(&output.stderr).unwrap()
        );
    }

    fs::remove_file("shaders.air");
}
