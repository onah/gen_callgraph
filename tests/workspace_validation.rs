use gen_callgraph::cli::validate_rust_workspace;

#[test]
fn validate_workspace_rejects_directory_without_cargo_toml() {
    let dir = std::env::temp_dir().join("gen_callgraph_test_no_cargo_toml");
    std::fs::create_dir_all(&dir).unwrap();
    let _ = std::fs::remove_file(dir.join("Cargo.toml"));

    let result = validate_rust_workspace(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("Cargo.toml"),
        "error should mention Cargo.toml, got: {}",
        msg
    );
}

#[test]
fn validate_workspace_accepts_directory_with_cargo_toml() {
    let dir = std::env::temp_dir().join("gen_callgraph_test_valid_project");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

    let result = validate_rust_workspace(&dir);
    let _ = std::fs::remove_dir_all(&dir);

    assert!(result.is_ok());
}

#[test]
fn validate_workspace_rejects_file_path() {
    let file = std::env::temp_dir().join("gen_callgraph_test_not_a_dir.txt");
    std::fs::write(&file, "test content").unwrap();

    let result = validate_rust_workspace(&file);
    let _ = std::fs::remove_file(&file);

    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not a directory"),
        "error should say 'not a directory', got: {}",
        msg
    );
}
