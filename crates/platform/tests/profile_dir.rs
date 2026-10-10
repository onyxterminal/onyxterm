use platform::{database_path, profile_dir, DATABASE_FILE_NAME, PROFILE_DIR_ENV};
use std::path::PathBuf;

#[test]
fn profile_dir_honours_env_override_and_defaults_per_platform() {
    let override_dir = std::env::temp_dir().join("onyxterm-profile-override");
    std::env::set_var(PROFILE_DIR_ENV, &override_dir);
    assert_eq!(profile_dir("default").expect("override"), override_dir);
    assert_eq!(
        database_path("default").expect("db path"),
        override_dir.join(DATABASE_FILE_NAME)
    );

    std::env::remove_var(PROFILE_DIR_ENV);
    let resolved = profile_dir("work").expect("default dir");
    let expected_suffix = if cfg!(target_os = "macos") {
        PathBuf::from("Library/Application Support/Onyxterm/work")
    } else {
        PathBuf::from("onyxterm/work")
    };
    assert!(resolved.ends_with(&expected_suffix), "{}", resolved.display());
}
