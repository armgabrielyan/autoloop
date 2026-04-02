use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn top_level_help_lists_command_descriptions() {
    let mut command = Command::cargo_bin("autoloop").expect("binary exists");
    command.arg("--help");

    command.assert().success().stdout(
        predicate::str::contains("Initialize AutoLoop in the current repository").and(
            predicate::str::contains("Refresh learnings from recorded experiment history"),
        ),
    );
}

#[test]
fn learn_help_lists_option_descriptions() {
    let mut command = Command::cargo_bin("autoloop").expect("binary exists");
    command.args(["learn", "--help"]);

    command.assert().success().stdout(
        predicate::str::contains("Refresh learnings from recorded experiment history")
            .and(predicate::str::contains(
                "Limit learnings to the latest completed session",
            ))
            .and(predicate::str::contains(
                "Aggregate learnings across all recorded sessions",
            )),
    );
}
