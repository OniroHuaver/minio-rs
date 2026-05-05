//! OS utility tests
//!
//! Tests readDir/readDirN, mkdirAll, renameAll filesystem operations.

/// Test readDir with various error conditions
#[test]
#[ignore]
fn test_read_dir_fail() {
    // TODO: implement when readDir function available
    //
    // Steps:
    //   1. /tmp/non-existent-directory -> errFileNotFound
    //   2. File path + "/mydir" -> errFileNotFound
    //   3. Linux only: no read permission directory -> error
}

/// Test readDir reads empty directory
#[test]
#[ignore]
fn test_read_dir_empty() {
    // TODO: implement when readDir available
    //
    // Steps: Create temp empty directory -> readDir -> empty list
}

/// Test readDir reads directory with files only
#[test]
#[ignore]
fn test_read_dir_files() {
    // TODO: implement when readDir available
    //
    // Steps: Create 10 files -> readDir -> sorted filenames verified
}

/// Test readDir reads directory with files and subdirectories
#[test]
#[ignore]
fn test_read_dir_generic() {
    // TODO: implement when readDir available
    //
    // Steps: Create mydir/ and 10 files -> readDir -> contains "mydir/"
}

/// Test readDir reads directory with symlinks
#[test]
#[ignore]
fn test_read_dir_symlink() {
    // TODO: implement when readDir available
    //
    // Steps:
    //   Create 10 files, 10 symlinks, 1 directory
    //   readDir -> lists all files and symlinks (including "mydir/")
    //   Symlinks preserved (platform-dependent)
}

/// Test readDirN reads N entries
#[test]
#[ignore]
fn test_read_dir_n() {
    // TODO: implement when readDirN available
    //
    // Steps (11 test cases):
    //   (numFiles=0, n=0) -> 0
    //   (0, 1) -> 0
    //   (1, 0) -> 0
    //   (0, -1) -> 0
    //   (1, -1) -> 1
    //   (10, -1) -> 10
    //   (1, 1) -> 1
    //   (2, 1) -> 1
    //   (10, 9) -> 9
    //   (10, 10) -> 10
    //   (10, 11) -> 10
}

/// Test mkdirAll creates directories
#[test]
#[ignore]
fn test_os_mkdir_all() {
    // TODO: implement when mkdirAll + pathJoin available
    //
    // Steps:
    //   1. mkdirAll("", 0777, "") -> errInvalidArgument
    //   2. mkdirAll(extremely_long_path, ...) -> errFileNameTooLong
    //   3. mkdirAll("success-vol/success-object", ...) -> Ok
}

/// Test renameAll rename/move
#[test]
#[ignore]
fn test_os_rename_all() {
    // TODO: implement when renameAll available
    //
    // Steps:
    //   1. renameAll("", "foo", "") -> errInvalidArgument
    //   2. renameAll("foo", "", "") -> errInvalidArgument
    //   3. renameAll(src, dst) -> Ok
    //   4. renameAll(already moved src, dst) -> errFileNotFound
    //   5. Path too long -> errFileNameTooLong
    //   6. Target too long -> errFileNameTooLong
}
