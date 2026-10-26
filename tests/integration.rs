#[cfg(not(target_os = "windows"))]
use git2::Commit;
use git2::{IndexAddOption, Oid, Repository, RepositoryInitOptions};
#[cfg(not(target_os = "windows"))]
use rexpect::{
    process::wait,
    session::{spawn_command, PtySession},
};
use std::{error::Error, fs, path::PathBuf, process::Command};
use tempfile::TempDir;

#[cfg(not(target_os = "windows"))]
fn setup_config_home() -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    std::env::set_var("XDG_CONFIG_HOME", temp_dir.path());

    Ok(temp_dir)
}

fn setup_test_dir() -> Result<(PathBuf, TempDir, Repository), Box<dyn Error>> {
    let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
    let temp_dir = tempfile::tempdir()?;
    let mut init_options = RepositoryInitOptions::new();
    init_options.initial_head("main");
    let repo = Repository::init_opts(&temp_dir, &init_options)?;

    let mut gitconfig = repo.config()?;
    gitconfig.set_str("user.name", "test")?;
    gitconfig.set_str("user.email", "test@example.org")?;

    Ok((bin_path, temp_dir, repo))
}

#[cfg(not(target_os = "windows"))]
fn get_last_commit(repo: &Repository) -> Result<Commit<'_>, git2::Error> {
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    let oid = walk.next().expect("cannot get commit in revwalk")?;

    repo.find_commit(oid)
}

fn do_initial_commit(repo: &Repository, message: &'static str) -> Result<Oid, git2::Error> {
    let signature = repo.signature()?;
    let oid = repo.index()?.write_tree()?;
    let tree = repo.find_tree(oid)?;

    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
}

#[cfg(not(target_os = "windows"))]
trait ExpectPromps {
    fn expect_commit_type(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_scope(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_summary(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_body(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_breaking(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_breaking_details(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_issues(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_issues_details(&mut self) -> Result<String, rexpect::error::Error>;
    fn expect_confirm(&mut self) -> Result<String, rexpect::error::Error>;
}

#[cfg(not(target_os = "windows"))]
impl ExpectPromps for PtySession {
    fn expect_commit_type(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("are you committing?")
    }

    fn expect_scope(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("scope of this change?")
    }

    fn expect_summary(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("description of the change")
    }

    fn expect_body(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("longer description of the change")
    }

    fn expect_breaking(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("breaking changes?")
    }

    fn expect_breaking_details(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("in detail:")
    }

    fn expect_issues(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("any open issues?")
    }

    fn expect_issues_details(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("issue reference:")
    }

    fn expect_confirm(&mut self) -> Result<String, rexpect::error::Error> {
        self.exp_string("Proceed with this commit?")
    }
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_everything_correct() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("README.md"), "foo")?;
    let mut index = repo.index()?;
    index.add_all(["."].iter(), IndexAddOption::default(), None)?;
    index.write()?;
    do_initial_commit(&repo, "docs(readme): initial draft")?;

    fs::write(temp_dir.path().join("config.json"), "bar")?;
    // TODO properly test "-a"
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("-a")
        .arg("-y")
        .arg("--autocomplete=true");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("feat")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("config")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("refactor config pairs")?;
    process.flush()?;
    process.expect_body()?;
    process
        .send_line("Removed and added a config pair each\\nNecessary for future compatibility.")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("Y")?;
    process.flush()?;
    process.expect_breaking_details()?;
    process.send_line("Something can't be configured anymore")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("Y")?;
    process.flush()?;
    process.expect_issues_details()?;
    process.send_line("closes #1")?;
    process.flush()?;
    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    let commit = get_last_commit(&repo)?;
    assert_eq!(
        commit.summary(),
        Some("feat(config)!: refactor config pairs")
    );
    assert_eq!(
        commit.body(),
        Some("Removed and added a config pair each\nNecessary for future compatibility.\n\ncloses #1\nBREAKING CHANGE: Something can't be configured anymore")
    );

    temp_dir.close()?;
    config_temp_dir.close()?;
    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_hook_correct() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("config.json"), "abc")?;
    repo.index()?
        .add_all(["*"].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;
    fs::remove_file(temp_dir.path().join(".git").join("COMMIT_EDITMSG")).unwrap_or(());

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("--hook")
        .arg("-y")
        .arg("--autocomplete=true");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("fix")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("some weird error")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("N")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;
    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    let editmsg = temp_dir.path().join(".git").join("COMMIT_EDITMSG");
    assert!(editmsg.exists());
    assert_eq!(
        fs::read(editmsg)?,
        "fix: some weird error".as_bytes().to_vec()
    );

    temp_dir.close()?;
    config_temp_dir.close()?;
    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_stdout_correct() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("config.json"), "abc")?;
    repo.index()?
        .add_all(["*"].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;
    fs::remove_file(temp_dir.path().join(".git").join("COMMIT_EDITMSG")).unwrap_or(());

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("--stdout")
        .arg("--autocomplete=true");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("fix")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("some weird error")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("N")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;

    let expected_output = "fix: some weird error";

    let _ = process
        .exp_string(expected_output)
        .expect("failed to match output");

    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    let editmsg = temp_dir.path().join(".git").join("COMMIT_EDITMSG");
    assert!(!editmsg.exists());

    temp_dir.close()?;
    config_temp_dir.close()?;
    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_empty_breaking_text_correct() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("Cargo.toml"), "bar")?;
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("-a")
        .arg("-y")
        .arg("--autocomplete=true");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("docs")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("cargo")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("rename project")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("Renamed the project to a new name.")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("Y")?;
    process.flush()?;
    process.expect_breaking_details()?;
    // `^[` is the same as <esc>
    process.send_control('[')?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;
    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    let commit = get_last_commit(&repo)?;
    assert_eq!(commit.summary(), Some("docs(cargo)!: rename project"));
    assert_eq!(commit.body(), Some("Renamed the project to a new name."));

    temp_dir.close()?;
    config_temp_dir.close()?;
    Ok(())
}

#[test]
fn test_non_repository_error() -> Result<(), Box<dyn Error>> {
    let bin_path = assert_cmd::cargo::cargo_bin!("koji");
    let temp_dir = tempfile::tempdir()?;

    let mut cmd = Command::new(bin_path);
    cmd.arg("-C").arg(temp_dir.path());

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(stderr_out.contains("could not find git repository"));

    temp_dir.close()?;
    Ok(())
}

#[test]
fn test_empty_repository_error() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, _) = setup_test_dir()?;

    let mut cmd = Command::new(bin_path);
    cmd.arg("-C").arg(temp_dir.path()).arg("-y");

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(stderr_out.contains("no files staged for commit"));

    temp_dir.close()?;
    Ok(())
}

#[test]
fn test_all_hook_exclusive_error() -> Result<(), Box<dyn Error>> {
    let bin_path = assert_cmd::cargo::cargo_bin!("koji");

    let mut cmd = Command::new(bin_path);
    cmd.arg("--hook");
    cmd.arg("--all");

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(stderr_out.contains("the argument '--hook' cannot be used with '--all'"));

    Ok(())
}

#[test]
fn test_all_stdout_exclusive_error() -> Result<(), Box<dyn Error>> {
    let bin_path = assert_cmd::cargo::cargo_bin!("koji");

    let mut cmd = Command::new(bin_path);
    cmd.arg("--stdout");
    cmd.arg("--all");

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(stderr_out.contains("the argument '--stdout' cannot be used with '--all'"));

    Ok(())
}

#[test]
fn test_hook_stdout_exclusive_error() -> Result<(), Box<dyn Error>> {
    let bin_path = assert_cmd::cargo::cargo_bin!("koji");

    let mut cmd = Command::new(bin_path);
    cmd.arg("--stdout");
    cmd.arg("--hook");

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(stderr_out.contains("the argument '--stdout' cannot be used with '--hook'"));

    Ok(())
}

#[test]
fn test_completion_scripts_success() -> Result<(), Box<dyn Error>> {
    fn run_for(shell: &'static str, containing: &'static str) -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji");

        let mut cmd = Command::new(bin_path);
        cmd.arg("completions").arg(shell);

        let cmd_out = cmd.output()?;
        let stdout = String::from_utf8(cmd_out.stdout)?;

        assert!(cmd_out.status.success());
        assert!(stdout.contains(containing));

        Ok(())
    }

    run_for("nushell", "def \"nu-complete koji")?;
    run_for("fish", "complete -c koji -n \"__fish_koji_needs_command")?;
    run_for("bash", "complete -F _koji -o bashdefault -o default koji")?;
    run_for(
        "powershell",
        "Register-ArgumentCompleter -Native -CommandName 'koji'",
    )?;
    run_for("zsh", "#compdef koji")
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_xdg_config() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    let xdg_cfg_home = tempfile::tempdir()?;
    fs::create_dir(xdg_cfg_home.path().join("koji"))?;
    fs::write(
        xdg_cfg_home.path().join("koji/config.toml"),
        "[[commit_types]]\nname=\"wip\"\ndescription = \"Do not create PR with this commit\"",
    )?;

    fs::write(temp_dir.path().join("config.json"), "bar")?;
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .env("XDG_CONFIG_HOME", xdg_cfg_home.path().as_os_str())
        .arg("-C")
        .arg(temp_dir.path())
        .arg("--stdout")
        .arg("--autocomplete=true");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("wip")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("some weird error")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("N")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;

    let expected_output = "wip: some weird error";

    let _ = process
        .exp_string(expected_output)
        .expect("failed to match output");

    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {:?}", eof_output);
    }

    temp_dir.close()?;
    config_temp_dir.close()?;
    xdg_cfg_home.close()?;

    Ok(())
}

#[test]
fn test_no_staged_files_error() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;

    fs::write(temp_dir.path().join("README.md"), "hello")?;
    let mut index = repo.index()?;
    index.add_all(["."].iter(), IndexAddOption::default(), None)?;
    index.write()?;
    do_initial_commit(&repo, "docs: initial")?;

    // Modify a file but don't stage it
    fs::write(temp_dir.path().join("README.md"), "changed")?;

    let mut cmd = Command::new(bin_path);
    cmd.arg("-C").arg(temp_dir.path());

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!cmd_out.status.success());
    assert!(
        stderr_out.contains("no files staged for commit"),
        "expected 'no files staged for commit' in stderr, got: {stderr_out}"
    );

    temp_dir.close()?;

    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_confirmation_accept() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("README.md"), "foo")?;
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("-a");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("fix")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("patch a bug")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("N")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;

    // Expect the commit message preview and confirmation prompt
    process.exp_string("fix: patch a bug")?;
    process.expect_confirm()?;
    process.send_line("Y")?;
    process.flush()?;
    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    let commit = get_last_commit(&repo)?;
    assert_eq!(commit.summary(), Some("fix: patch a bug"));

    temp_dir.close()?;
    config_temp_dir.close()?;

    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_partial_staging_warning() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;

    fs::write(temp_dir.path().join("a.txt"), "aaa")?;
    fs::write(temp_dir.path().join("b.txt"), "bbb")?;
    let mut index = repo.index()?;
    index.add_all(["."].iter(), IndexAddOption::default(), None)?;
    index.write()?;
    do_initial_commit(&repo, "chore: initial")?;

    // Modify both, stage only one
    fs::write(temp_dir.path().join("a.txt"), "aaa changed")?;
    fs::write(temp_dir.path().join("b.txt"), "bbb changed")?;
    let mut index = repo.index()?;
    index.add_all(["a.txt"].iter(), IndexAddOption::default(), None)?;
    index.write()?;

    let mut cmd = Command::new(&bin_path);
    cmd.arg("-C").arg(temp_dir.path());

    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(
        stderr_out.contains("file(s) staged for commit"),
        "expected partial staging warning in stderr, got: {stderr_out}"
    );
    assert!(
        stderr_out.contains("unstaged changes not included"),
        "expected unstaged warning in stderr, got: {stderr_out}"
    );
    assert!(!stderr_out.contains("no files staged for commit"));

    temp_dir.close()?;

    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_all_flag_skips_staging_check() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;

    fs::write(temp_dir.path().join("README.md"), "hello")?;
    let mut index = repo.index()?;
    index.add_all(["."].iter(), IndexAddOption::default(), None)?;
    index.write()?;
    do_initial_commit(&repo, "docs: initial")?;

    // Modify a file but don't stage it
    fs::write(temp_dir.path().join("README.md"), "changed")?;

    let mut cmd = Command::new(bin_path);
    cmd.arg("-C").arg(temp_dir.path()).arg("--all");
    let cmd_out = cmd.output()?;
    let stderr_out = String::from_utf8(cmd_out.stderr)?;

    assert!(!stderr_out.contains("no files staged for commit"));

    temp_dir.close()?;

    Ok(())
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_confirmation_decline() -> Result<(), Box<dyn Error>> {
    let (bin_path, temp_dir, repo) = setup_test_dir()?;
    let config_temp_dir = setup_config_home()?;

    fs::write(temp_dir.path().join("README.md"), "foo")?;
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    do_initial_commit(&repo, "docs(readme): initial draft")?;

    fs::write(temp_dir.path().join("config.json"), "bar")?;
    repo.index()?
        .add_all(["."].iter(), IndexAddOption::default(), None)?;
    repo.index()?.write()?;

    let mut cmd = Command::new(bin_path);
    cmd.env("NO_COLOR", "1")
        .arg("-C")
        .arg(temp_dir.path())
        .arg("-a");

    let mut process = spawn_command(cmd, Some(5000))?;

    process.expect_commit_type()?;
    process.send_line("fix")?;
    process.flush()?;
    process.expect_scope()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_summary()?;
    process.send_line("patch a bug")?;
    process.flush()?;
    process.expect_body()?;
    process.send_line("")?;
    process.flush()?;
    process.expect_breaking()?;
    process.send_line("N")?;
    process.flush()?;
    process.expect_issues()?;
    process.send_line("N")?;
    process.flush()?;

    // Expect the commit message preview and decline the confirmation
    process.exp_string("fix: patch a bug")?;
    process.expect_confirm()?;
    process.send_line("N")?;
    process.flush()?;
    let eof_output = process.exp_eof();

    let exitcode = process.process.wait()?;
    let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

    if !success {
        panic!("Command exited non-zero, end of output: {eof_output:#?}");
    }

    // Verify the last commit is still the initial one (no new commit was made)
    let commit = get_last_commit(&repo)?;
    assert_eq!(commit.summary(), Some("docs(readme): initial draft"));

    temp_dir.close()?;
    config_temp_dir.close()?;

    Ok(())
}

// ---- Jj integration tests ----

#[cfg(feature = "jj")]
mod jj_tests {
    use super::*;
    use std::path::Path;

    use jj_lib::config::StackedConfig;
    use jj_lib::default_backend_factories::{
        default_backend_factories, default_working_copy_factories, default_working_copy_factory,
    };
    use jj_lib::object_id::ObjectId as _;
    use jj_lib::repo::{ReadonlyRepo, Repo as _};
    use jj_lib::settings::UserSettings;
    use jj_lib::workspace::Workspace;
    use pollster::FutureExt as _;
    use std::sync::Arc;

    fn jj_settings() -> UserSettings {
        let config = StackedConfig::with_defaults();
        UserSettings::from_config_and_home_dir(config, None)
            .expect("could not create jj UserSettings")
    }

    /// Create a non-colocated jj repo in the given directory.
    fn create_jj_repo(dir: &Path) {
        let settings = jj_settings();
        Workspace::init_internal_git(&settings, dir, Default::default())
            .block_on()
            .expect("failed to init non-colocated jj repo");
    }

    /// Create a colocated jj repo (both `.jj/` and `.git/` exist).
    fn create_colocated_jj_repo(dir: &Path) {
        let settings = jj_settings();
        Workspace::init_colocated_git(&settings, dir, Default::default())
            .block_on()
            .expect("failed to init colocated jj repo");
    }

    fn jj_load_repo(
        workspace_root: &Path,
    ) -> (Arc<ReadonlyRepo>, jj_lib::ref_name::WorkspaceNameBuf) {
        let settings = jj_settings();
        let store_factories = default_backend_factories();
        let wc_factories = default_working_copy_factories();
        let workspace = Workspace::load(&settings, workspace_root, &store_factories, &wc_factories)
            .expect("could not load jj workspace");
        let workspace_name = workspace.workspace_name().to_owned();
        let repo = workspace
            .repo_loader()
            .load_at_head()
            .block_on()
            .expect("could not load jj repo at head");
        (repo, workspace_name)
    }

    fn jj_wc_commit(
        repo: &Arc<ReadonlyRepo>,
        workspace_name: &jj_lib::ref_name::WorkspaceName,
    ) -> jj_lib::commit::Commit {
        let wc_commit_id = repo
            .view()
            .get_wc_commit_id(workspace_name)
            .expect("no working copy commit found");
        repo.store()
            .get_commit(wc_commit_id)
            .expect("could not get working copy commit")
    }

    /// Get the current `@` commit description via jj-lib.
    fn jj_get_description(dir: &Path) -> String {
        let (repo, workspace_name) = jj_load_repo(dir);
        let commit = jj_wc_commit(&repo, &workspace_name);
        commit.description().trim().to_string()
    }

    /// Describe the current `@` commit (for pre-population tests).
    fn jj_describe(dir: &Path, message: &str) {
        let (repo, workspace_name) = jj_load_repo(dir);
        let commit = jj_wc_commit(&repo, &workspace_name);
        let mut tx = repo.start_transaction();
        tx.repo_mut()
            .rewrite_commit(&commit)
            .set_description(message)
            .write()
            .block_on()
            .expect("could not write jj commit description");
        tx.repo_mut()
            .rebase_descendants()
            .block_on()
            .expect("could not rebase descendants");
        tx.commit("test: describe change")
            .block_on()
            .expect("could not commit jj transaction");
    }

    fn create_jj_workspace(existing_workspace_root: &Path, new_workspace_root: &Path, name: &str) {
        std::fs::create_dir_all(new_workspace_root).expect("failed to create workspace directory");

        let settings = jj_settings();
        let store_factories = default_backend_factories();
        let wc_factories = default_working_copy_factories();
        let existing_workspace = Workspace::load(
            &settings,
            existing_workspace_root,
            &store_factories,
            &wc_factories,
        )
        .expect("could not load existing workspace");
        let repo = existing_workspace
            .repo_loader()
            .load_at_head()
            .block_on()
            .expect("could not load repo at head");

        Workspace::init_workspace_with_existing_repo(
            new_workspace_root,
            existing_workspace.repo_path(),
            &repo,
            &*default_working_copy_factory(),
            name.into(),
        )
        .block_on()
        .expect("failed to create workspace");
    }

    // ---- Non-interactive tests ----

    #[test]
    fn test_jj_commit_preserves_workspace_git_heads() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempfile::tempdir()?;
        let main_dir = temp_dir.path().join("main");
        let secondary_dir = temp_dir.path().join("secondary");
        fs::create_dir(&main_dir)?;
        create_colocated_jj_repo(&main_dir);
        fs::write(main_dir.join("README.md"), "initial\n")?;
        let backend = koji::vcs::VcsBackend::detect_with_hint(&main_dir, None)?;
        backend.write_commit_msg("feat".into(), None, "initial".into(), None, false)?;

        let (repo, workspace_name) = jj_load_repo(&main_dir);
        let working_copy = jj_wc_commit(&repo, &workspace_name);
        let git_repo = Repository::open(&main_dir)?;
        assert_eq!(
            git_repo.head()?.target().unwrap().as_bytes(),
            working_copy.parent_ids()[0].as_bytes()
        );
        assert_eq!(
            git_repo.status_file(Path::new("README.md"))?,
            git2::Status::CURRENT
        );
        assert_eq!(
            repo.view().git_head(&workspace_name).as_normal(),
            Some(&working_copy.parent_ids()[0])
        );
        let git_heads = repo.view().all_git_heads().clone();

        create_jj_workspace(&main_dir, &secondary_dir, "secondary");
        fs::write(secondary_dir.join("secondary.txt"), "secondary\n")?;
        let backend = koji::vcs::VcsBackend::detect_with_hint(&secondary_dir, None)?;
        backend.write_commit_msg("feat".into(), None, "secondary".into(), None, false)?;

        let (repo, secondary_name) = jj_load_repo(&secondary_dir);
        assert_eq!(repo.view().all_git_heads(), &git_heads);
        assert_eq!(jj_wc_commit(&repo, &workspace_name).id(), working_copy.id());
        let secondary = jj_wc_commit(&repo, &secondary_name);
        assert!(secondary.description().is_empty());
        assert_eq!(
            repo.store()
                .get_commit(&secondary.parent_ids()[0])?
                .description()
                .trim(),
            "feat: secondary"
        );
        assert_eq!(
            git_repo.head()?.target().unwrap().as_bytes(),
            working_copy.parent_ids()[0].as_bytes()
        );
        Ok(())
    }

    #[test]
    fn test_jj_commit_leaves_unrelated_git_repository_untouched() -> Result<(), Box<dyn Error>> {
        let (_, temp_dir, git_repo) = setup_test_dir()?;
        let git_head = do_initial_commit(&git_repo, "unrelated Git commit")?;
        create_jj_repo(temp_dir.path());
        fs::write(temp_dir.path().join("README.md"), "hello\n")?;
        let mut index = git_repo.index()?;
        index.add_path(Path::new("README.md"))?;
        index.write()?;
        let git_tree = index.write_tree()?;

        let backend = koji::vcs::VcsBackend::detect_with_hint(temp_dir.path(), None)?;
        backend.write_commit_msg("feat".into(), None, "add readme".into(), None, false)?;

        assert_eq!(git_repo.head()?.target(), Some(git_head));
        assert_eq!(git_repo.index()?.write_tree()?, git_tree);
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("README.md"))?,
            "hello\n"
        );
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        let working_copy = jj_wc_commit(&repo, &workspace_name);
        assert_eq!(
            repo.store()
                .get_commit(&working_copy.parent_ids()[0])?
                .description()
                .trim(),
            "feat: add readme"
        );
        Ok(())
    }

    #[test]
    fn test_jj_commit_reports_invalid_colocation_metadata() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempfile::tempdir()?;
        create_jj_repo(temp_dir.path());
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        let original = jj_wc_commit(&repo, &workspace_name);
        Repository::init(temp_dir.path())?;
        fs::write(temp_dir.path().join(".git/config"), "[invalid\n")?;

        let backend = koji::vcs::VcsBackend::detect_with_hint(temp_dir.path(), None)?;
        let err = backend
            .write_commit_msg("feat".into(), None, "add readme".into(), None, false)
            .unwrap_err();

        assert!(err
            .to_string()
            .contains("could not determine whether jj workspace is colocated"));
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        assert_eq!(jj_wc_commit(&repo, &workspace_name).id(), original.id());
        Ok(())
    }

    #[test]
    fn test_jj_commit_reports_git_import_export_lock_failure() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempfile::tempdir()?;
        create_colocated_jj_repo(temp_dir.path());
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        let original = jj_wc_commit(&repo, &workspace_name);
        fs::create_dir(temp_dir.path().join(".jj/repo/git_import_export.lock"))?;

        let backend = koji::vcs::VcsBackend::detect_with_hint(temp_dir.path(), None)?;
        let err = backend
            .write_commit_msg("feat".into(), None, "add readme".into(), None, false)
            .unwrap_err();

        assert!(err.to_string().contains("could not lock Git import/export"));
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        assert_eq!(jj_wc_commit(&repo, &workspace_name).id(), original.id());
        Ok(())
    }

    #[test]
    fn test_jj_commit_preserves_concurrently_updated_git_head() -> Result<(), Box<dyn Error>> {
        let temp_dir = tempfile::tempdir()?;
        create_colocated_jj_repo(temp_dir.path());
        fs::write(temp_dir.path().join("README.md"), "hello\n")?;
        let backend = koji::vcs::VcsBackend::detect_with_hint(temp_dir.path(), None)?;
        backend.write_commit_msg("feat".into(), None, "add readme".into(), None, false)?;

        let git_repo = Repository::open(temp_dir.path())?;
        git_repo.config()?.set_str("user.name", "test")?;
        git_repo
            .config()?
            .set_str("user.email", "test@example.org")?;
        let parent = git_repo.head()?.peel_to_commit()?;
        let signature = git_repo.signature()?;
        let concurrent_head = git_repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            "concurrent Git commit",
            &parent.tree()?,
            &[&parent],
        )?;

        backend.write_commit_msg("feat".into(), None, "second change".into(), None, false)?;

        assert_eq!(git_repo.head()?.target(), Some(concurrent_head));
        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        let working_copy = jj_wc_commit(&repo, &workspace_name);
        assert!(working_copy.description().is_empty());
        assert_eq!(
            repo.store()
                .get_commit(&working_copy.parent_ids()[0])?
                .description()
                .trim(),
            "feat: second change"
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires an installed jj CLI; run with --ignored --nocapture"]
    fn test_jj_commit_with_installed_cli() -> Result<(), Box<dyn Error>> {
        fn jj(dir: &Path, args: &[&str]) -> String {
            let output = Command::new("jj")
                .current_dir(dir)
                .args(["--no-pager", "--config=ui.color=never"])
                .args(args)
                .output()
                .expect("could not run installed jj CLI");
            assert!(
                output.status.success(),
                "jj {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).expect("jj output was not UTF-8")
        }

        let temp_dir = tempfile::tempdir()?;
        println!("{}", jj(temp_dir.path(), &["--version"]));
        for name in [
            "reference",
            "koji",
            "reference-colocated",
            "koji-colocated",
            "reference-workspace",
            "koji-workspace",
        ] {
            let dir = temp_dir.path().join(name);
            fs::create_dir(&dir)?;
            if name.ends_with("-colocated") || name.ends_with("-workspace") {
                jj(&dir, &["git", "init", "--colocate"]);
            } else {
                jj(&dir, &["git", "init"]);
            }
            fs::write(dir.join("README.md"), "initial\n")?;
            jj(&dir, &["commit", "-m", "initial commit"]);
            let dir = if name.ends_with("-workspace") {
                let workspace_dir = dir.join("secondary");
                jj(&dir, &["workspace", "add", "--colocate", "secondary"]);
                workspace_dir
            } else {
                dir
            };
            fs::write(dir.join("README.md"), "hello\n")?;
            if name.starts_with("reference") {
                jj(&dir, &["commit", "-m", "feat(core): add readme"]);
            } else {
                let backend = koji::vcs::VcsBackend::detect_with_hint(&dir, None)?;
                backend.write_commit_msg(
                    "feat".into(),
                    Some("core".into()),
                    "add readme".into(),
                    None,
                    false,
                )?;
            }
            println!("{name}:\n{}", jj(&dir, &["log", "-r", "ancestors(@, 3)"]));
            assert_eq!(
                jj(
                    &dir,
                    &[
                        "log",
                        "--no-graph",
                        "-r",
                        "@",
                        "-T",
                        r#"description ++ "|" ++ parents.map(|p| p.description()).join(",")"#,
                    ],
                )
                .trim(),
                "|feat(core): add readme"
            );
            assert_eq!(
                jj(&dir, &["file", "show", "-r", "@-", "README.md"]),
                "hello\n"
            );
            assert_eq!(fs::read_to_string(dir.join("README.md"))?, "hello\n");
            assert!(jj(&dir, &["diff", "--summary"]).is_empty());
        }
        Ok(())
    }

    #[test]
    fn test_jj_repo_detection() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;

        create_jj_repo(temp_dir.path());

        let output = Command::new(&bin_path)
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--hook")
            .output()?;

        let stderr = String::from_utf8(output.stderr)?;
        assert!(!output.status.success());
        assert!(
            stderr.contains("--hook mode is not supported with jj"),
            "expected jj hook rejection, got: {stderr}"
        );

        temp_dir.close()?;
        Ok(())
    }

    #[test]
    fn test_colocated_prefers_jj() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;

        create_colocated_jj_repo(temp_dir.path());

        assert!(temp_dir.path().join(".jj").is_dir());
        assert!(temp_dir.path().join(".git").is_dir());

        let output = Command::new(&bin_path)
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--hook")
            .output()?;

        let stderr = String::from_utf8(output.stderr)?;
        assert!(!output.status.success());
        assert!(
            stderr.contains("--hook mode is not supported with jj"),
            "expected jj backend chosen for colocated repo, got: {stderr}"
        );

        temp_dir.close()?;
        Ok(())
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_colocated_config_override_to_git() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;
        let config_temp_dir = setup_config_home()?;

        create_colocated_jj_repo(temp_dir.path());

        fs::write(temp_dir.path().join(".koji.toml"), "vcs = \"git\"")?;

        // Stage a file so the git backend doesn't complain about empty staging
        fs::write(temp_dir.path().join("test.txt"), "hello")?;
        let repo = Repository::open(temp_dir.path())?;
        let mut index = repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::default(), None)?;
        index.write()?;

        // --hook mode should work (not rejected) because git backend is forced.
        let output = Command::new(&bin_path)
            .env("NO_COLOR", "1")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--hook")
            .output()?;

        let stderr = String::from_utf8(output.stderr)?;
        assert!(
            !stderr.contains("--hook mode is not supported with jj"),
            "expected git backend due to config override, but jj was chosen: {stderr}"
        );

        temp_dir.close()?;
        config_temp_dir.close()?;
        Ok(())
    }

    // ---- Interactive tests (require PTY, not supported on Windows) ----

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_jj_commit() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;
        let config_temp_dir = setup_config_home()?;

        create_jj_repo(temp_dir.path());

        fs::write(temp_dir.path().join("README.md"), "hello")?;

        let mut cmd = Command::new(&bin_path);
        cmd.env("NO_COLOR", "1")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("-y")
            .arg("--autocomplete=true");

        let mut process = spawn_command(cmd, Some(10000))?;

        process.expect_commit_type()?;
        process.send_line("feat")?;
        process.flush()?;
        process.expect_scope()?;
        process.send_line("core")?;
        process.flush()?;
        process.expect_summary()?;
        process.send_line("add readme")?;
        process.flush()?;
        process.expect_body()?;
        process.send_line("")?;
        process.flush()?;
        process.expect_breaking()?;
        process.send_line("N")?;
        process.flush()?;
        process.expect_issues()?;
        process.send_line("N")?;
        process.flush()?;
        let eof_output = process.exp_eof();

        let exitcode = process.process.wait()?;
        let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

        if !success {
            panic!("Command exited non-zero, end of output: {eof_output:#?}");
        }

        let (repo, workspace_name) = jj_load_repo(temp_dir.path());
        let working_copy = jj_wc_commit(&repo, &workspace_name);
        assert!(working_copy.description().is_empty());
        assert_eq!(working_copy.parent_ids().len(), 1);
        let parent = repo.store().get_commit(&working_copy.parent_ids()[0])?;
        assert_ne!(working_copy.change_id(), parent.change_id());
        assert_eq!(working_copy.tree_ids(), parent.tree_ids());
        assert_ne!(parent.tree_ids(), &repo.store().empty_merged_tree_id());
        assert_eq!(
            fs::read_to_string(temp_dir.path().join("README.md"))?,
            "hello"
        );
        let description = parent.description().trim();
        assert_eq!(
            description, "feat(core): add readme",
            "jj commit did not describe the parent of the new working-copy change"
        );

        temp_dir.close()?;
        config_temp_dir.close()?;
        Ok(())
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_jj_commit_in_non_default_workspace() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;
        let config_temp_dir = setup_config_home()?;
        let second_workspace_path = temp_dir.path().join("secondary-workspace");

        create_jj_repo(temp_dir.path());
        create_jj_workspace(temp_dir.path(), &second_workspace_path, "secondary");

        fs::write(second_workspace_path.join("README.md"), "hello")?;

        let mut cmd = Command::new(&bin_path);
        cmd.env("NO_COLOR", "1")
            .arg("-C")
            .arg(&second_workspace_path)
            .arg("-y")
            .arg("--autocomplete=true");

        let mut process = spawn_command(cmd, Some(10000))?;

        process.expect_commit_type()?;
        process.send_line("feat")?;
        process.flush()?;
        process.expect_scope()?;
        process.send_line("workspace")?;
        process.flush()?;
        process.expect_summary()?;
        process.send_line("describe from non-default workspace")?;
        process.flush()?;
        process.expect_body()?;
        process.send_line("")?;
        process.flush()?;
        process.expect_breaking()?;
        process.send_line("N")?;
        process.flush()?;
        process.expect_issues()?;
        process.send_line("N")?;
        process.flush()?;
        let eof_output = process.exp_eof();

        let exitcode = process.process.wait()?;
        let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

        if !success {
            panic!("Command exited non-zero, end of output: {eof_output:#?}");
        }

        let (repo, workspace_name) = jj_load_repo(&second_workspace_path);
        let working_copy = jj_wc_commit(&repo, &workspace_name);
        assert!(working_copy.description().is_empty());
        assert_eq!(working_copy.parent_ids().len(), 1);
        let parent = repo.store().get_commit(&working_copy.parent_ids()[0])?;
        assert_eq!(working_copy.tree_ids(), parent.tree_ids());
        assert_ne!(parent.tree_ids(), &repo.store().empty_merged_tree_id());
        assert!(jj_get_description(temp_dir.path()).is_empty());
        let description = parent.description().trim();
        assert_eq!(
            description, "feat(workspace): describe from non-default workspace",
            "jj commit did not describe the parent in non-default workspace"
        );

        temp_dir.close()?;
        config_temp_dir.close()?;
        Ok(())
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_jj_stdout_mode() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;
        let config_temp_dir = setup_config_home()?;

        create_jj_repo(temp_dir.path());
        fs::write(temp_dir.path().join("test.txt"), "content")?;

        let mut cmd = Command::new(&bin_path);
        cmd.env("NO_COLOR", "1")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--stdout")
            .arg("--autocomplete=true");

        let mut process = spawn_command(cmd, Some(10000))?;

        process.expect_commit_type()?;
        process.send_line("fix")?;
        process.flush()?;
        process.expect_scope()?;
        process.send_line("")?;
        process.flush()?;
        process.expect_summary()?;
        process.send_line("resolve issue")?;
        process.flush()?;
        process.expect_body()?;
        process.send_line("")?;
        process.flush()?;
        process.expect_breaking()?;
        process.send_line("N")?;
        process.flush()?;
        process.expect_issues()?;
        process.send_line("N")?;
        process.flush()?;

        let expected_output = "fix: resolve issue";
        let _ = process
            .exp_string(expected_output)
            .expect("failed to match output");

        let eof_output = process.exp_eof();

        let exitcode = process.process.wait()?;
        let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

        if !success {
            panic!("Command exited non-zero, end of output: {eof_output:#?}");
        }

        let description = jj_get_description(temp_dir.path());
        assert!(
            description.is_empty() || !description.contains("fix: resolve issue"),
            "stdout mode should not have written to jj, but description is: {description}"
        );

        temp_dir.close()?;
        config_temp_dir.close()?;
        Ok(())
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_jj_pre_population() -> Result<(), Box<dyn Error>> {
        let bin_path = assert_cmd::cargo::cargo_bin!("koji").to_path_buf();
        let temp_dir = tempfile::tempdir()?;
        let config_temp_dir = setup_config_home()?;

        create_jj_repo(temp_dir.path());
        fs::write(temp_dir.path().join("test.txt"), "content")?;

        jj_describe(temp_dir.path(), "docs(readme): update installation guide");

        // The existing description should be read and pre-populate the prompt.
        let mut cmd = Command::new(&bin_path);
        cmd.env("NO_COLOR", "1")
            .arg("-C")
            .arg(temp_dir.path())
            .arg("--stdout")
            .arg("--autocomplete=true");

        let mut process = spawn_command(cmd, Some(10000))?;

        // The prompt should appear; override the pre-populated values.
        // Fields with `with_initial_value` need to be cleared with backspaces
        // before typing new values since inquire appends to existing text.
        process.expect_commit_type()?;
        process.send_line("feat")?;
        process.flush()?;
        process.expect_scope()?;
        // Clear pre-populated scope ("readme" = 6 chars)
        for _ in 0..6 {
            process.send("\x7f")?;
        }
        process.send_line("api")?;
        process.flush()?;
        process.expect_summary()?;
        // Clear pre-populated summary ("update installation guide" = 25 chars)
        for _ in 0..25 {
            process.send("\x7f")?;
        }
        process.send_line("new endpoint")?;
        process.flush()?;
        process.expect_body()?;
        process.send_line("")?;
        process.flush()?;
        process.expect_breaking()?;
        process.send_line("N")?;
        process.flush()?;
        process.expect_issues()?;
        process.send_line("N")?;
        process.flush()?;

        let expected_output = "feat(api): new endpoint";
        let _ = process
            .exp_string(expected_output)
            .expect("failed to match output");

        let eof_output = process.exp_eof();

        let exitcode = process.process.wait()?;
        let success = matches!(exitcode, wait::WaitStatus::Exited(_, 0));

        if !success {
            panic!("Command exited non-zero, end of output: {eof_output:#?}");
        }

        temp_dir.close()?;
        config_temp_dir.close()?;
        Ok(())
    }
}
