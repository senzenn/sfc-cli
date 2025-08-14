use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::process::Command;
use tempfile::tempdir;
use std::fs;

fn bin() -> Command {
    Command::cargo_bin("sfc").unwrap()
}

#[test]
fn init_and_create_and_status_flow() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().join("ws");

    // init
    let mut cmd = bin();
    let assert = cmd.arg("init").arg(&root).assert();
    assert.success().stdout(predicate::str::contains("Initialized workspace"));

    // create
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("create").arg("demo1").assert().success();

    // status
    let mut cmd = bin();
    cmd.current_dir(&root);
    let out = cmd.arg("status").arg("demo1").assert();
    out.success().stdout(predicate::str::contains("Stable"));

    // temp
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("temp").arg("demo1").assert().success();

    // promote
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("promote").arg("demo1").assert().success();

    // discard (no-op likely)
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("discard").arg("demo1").assert().success();

    // clean
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("clean").assert().success();

    // rollback to current stable target
    let stable_link = root.join("links/demo1-stable");
    let rel = fs::read_link(&stable_link).unwrap();
    let snapshot_name = rel.file_name().unwrap().to_string_lossy().to_string();
    let mut cmd = bin();
    cmd.current_dir(&root);
    cmd.arg("rollback").arg("demo1").arg(snapshot_name).assert().success();
}


