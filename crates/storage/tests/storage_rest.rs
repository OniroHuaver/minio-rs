//! Storage REST client/server tests
//!
//! Tests storageRESTClient calling remote disks via REST API.
//! These are integration tests requiring a local HTTP server and client.

use storage::*;

/// Tests storageRESTClient.DiskInfo
///
/// Call remote DiskInfo via REST client, expected to return errUnformattedDisk.
#[test]
#[ignore]
fn test_storage_rest_client_disk_info() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // let result = client.disk_info(DiskInfoOptions { metrics: true }).await;
    // assert!(result.is_err());
    // assert_eq!(result.unwrap_err(), Error::UnformattedDisk);
}

/// Tests storageRESTClient.StatInfoFile
///
/// Scenarios:
/// - Existing file -> returns stats
/// - Non-existent file -> returns error
#[test]
#[ignore]
fn test_storage_rest_client_stat_info_file() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "myobject/xl.meta", b"foo").await?;
    //
    // let result = client.stat_info_file("foo", "myobject/xl.meta", false).await;
    // assert!(result.is_ok());
    //
    // let result = client.stat_info_file("foo", "yourobject/xl.meta", false).await;
    // assert!(result.is_err());
}

/// Tests storageRESTClient.ListDir
///
/// Scenarios:
/// - Existing directory -> returns subdirectory list
/// - Non-existent directory -> returns error
#[test]
#[ignore]
fn test_storage_rest_client_list_dir() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "path/to/myobject", b"foo").await?;
    //
    // let result = client.list_dir("", "foo", "path", -1).await;
    // assert_eq!(result.unwrap(), vec!["to/"]);
    //
    // let result = client.list_dir("", "foo", "nodir", -1).await;
    // assert!(result.is_err());
}

/// Tests storageRESTClient.ReadAll
///
/// Scenarios:
/// - Existing file -> returns correct content
/// - Non-existent file -> returns error
#[test]
#[ignore]
fn test_storage_rest_client_read_all() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "myobject", b"foo").await?;
    //
    // let data = client.read_all("foo", "myobject").await.unwrap();
    // assert_eq!(data, b"foo");
    //
    // let result = client.read_all("foo", "yourobject").await;
    // assert!(result.is_err());
}

/// Tests storageRESTClient.ReadFile
///
/// Scenarios:
/// - offset=0 -> returns full content
/// - offset=1 -> returns truncated content
/// - Non-existent file -> returns error
#[test]
#[ignore]
fn test_storage_rest_client_read_file() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "myobject", b"foo").await?;
    //
    // let mut buf = vec![0u8; 100];
    // let n = client.read_file("foo", "myobject", 0, &mut buf[..3], None).await.unwrap();
    // assert_eq!(&buf[..3], b"foo");
    //
    // let result = client.read_file("foo", "yourobject", 0, &mut buf, None).await;
    // assert!(result.is_err());
}

/// Tests storageRESTClient.AppendFile
///
/// Scenarios:
/// - Normal append -> verify content via ReadAll
/// - 0-byte data -> success
/// - Non-existent volume -> returns error
/// - Special characters (newline, tab, etc.) -> success
#[test]
#[ignore]
fn test_storage_rest_client_append_file() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    //
    // client.append_file("foo", "myobject", b"foo").await?;
    // let data = client.read_all("foo", "myobject").await.unwrap();
    // assert_eq!(data, b"foo");
    //
    // // 0-byte
    // client.append_file("foo", "myobject-0byte", b"").await?;
    //
    // // Non-existent volume
    // let result = client.append_file("foo-bar", "myobject", b"foo").await;
    // assert!(result.is_err());
    //
    // // Special characters
    // client.append_file("foo", "newline\n", b"foo").await?;
    // client.append_file("foo", "newline\t", b"foo").await?;
}

/// Tests storageRESTClient.Delete file deletion
///
/// Scenarios:
/// - Delete existing file -> success
/// - Delete non-existent file -> success (idempotent)
#[test]
#[ignore]
fn test_storage_rest_client_delete_file() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "myobject", b"foo").await?;
    // client.delete("foo", "myobject", DeleteOptions { recursive: false, immediate: false }).await?;
    // client.delete("foo", "myobject", DeleteOptions { recursive: false, immediate: false }).await?;
    // client.delete("foo", "yourobject", DeleteOptions { recursive: false, immediate: false }).await?;
}

/// Tests storageRESTClient.RenameFile file rename
///
/// Scenarios:
/// - Rename within same volume -> success
/// - Rename across volumes -> success
/// - Overwrite destination -> success
#[test]
#[ignore]
fn test_storage_rest_client_rename_file() {
    // TODO: implement when storageRESTClient is available
    // let client = new_storage_rest_client()?;
    // client.append_file("foo", "myobject", b"foo").await?;
    // client.append_file("foo", "otherobject", b"foo").await?;
    //
    // client.rename_file("foo", "myobject", "foo", "yourobject").await?;
    // client.rename_file("foo", "yourobject", "bar", "myobject").await?;
    // client.rename_file("foo", "otherobject", "bar", "myobject").await?; // overwrite
}
