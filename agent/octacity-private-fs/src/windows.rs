//! Windows DACL implementation for private agent state.

use std::{
  ffi::c_void,
  fs,
  mem::size_of,
  os::windows::ffi::OsStrExt as _,
  os::windows::fs::{MetadataExt as _, OpenOptionsExt as _},
  os::windows::io::AsRawHandle as _,
  path::Path,
  ptr,
};

use windows_sys::Win32::{
  Foundation::{CloseHandle, GetLastError, HANDLE, LocalFree},
  Security::{
    ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    Authorization::{
      ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
      SDDL_REVISION_1, SE_FILE_OBJECT,
    },
    DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation, GetTokenInformation, INHERIT_ONLY_ACE,
    IsWellKnownSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER, TokenUser, WinBuiltinAdministratorsSid, WinCreatorOwnerRightsSid, WinCreatorOwnerSid,
    WinLocalSystemSid,
  },
  Storage::FileSystem::{
    CreateDirectoryW, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FileIdInfo,
    GetFileInformationByHandleEx,
  },
  System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE,
    ACCESS_ALLOWED_COMPOUND_ACE_TYPE, ACCESS_ALLOWED_OBJECT_ACE_TYPE,
  },
  System::Threading::{GetCurrentProcess, OpenProcessToken},
};

use crate::FileIdentity;

// `P` blocks inherited grants; `OICI` gives files and subdirectories the same
// policy. LocalSystem and administrators retain machine-service recovery
// access without granting interactive users access to job material.
const PRIVATE_DIRECTORY_SDDL: &str = "D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
// Fixed service SID of Windows Modules Installer. Windows uses this principal
// as the owner of protected operating-system paths, including volume roots on
// some supported installations. Trust the exact SID, never the broader
// `NT SERVICE` authority.
const TRUSTED_INSTALLER_SID: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";

// Rights that let an untrusted principal delete an ancestor, delete one of its
// children, or rewrite the ACL which protects the path. Creating a sibling is
// deliberately absent: Windows create/write access does not replace an
// existing protected child. Keep the masks local because windows-sys groups
// several of them under unrelated feature modules.
const FILE_DELETE_CHILD: u32 = 0x0000_0040;
const DELETE: u32 = 0x0001_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const GENERIC_ALL: u32 = 0x1000_0000;
const ANCESTOR_REPLACEMENT_RIGHTS: u32 = FILE_DELETE_CHILD | DELETE | WRITE_DAC | WRITE_OWNER | GENERIC_ALL;

pub(super) fn create_private_directory(path: &Path) -> std::io::Result<()> {
  let path = wide(path)?;
  let sddl = PRIVATE_DIRECTORY_SDDL.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
  let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
  // SAFETY: both UTF-16 inputs are NUL-terminated and the returned local
  // allocation is held by `SecurityDescriptor` until CreateDirectoryW ends.
  let converted = unsafe {
    ConvertStringSecurityDescriptorToSecurityDescriptorW(
      sddl.as_ptr(),
      SDDL_REVISION_1,
      &mut descriptor,
      ptr::null_mut(),
    )
  };
  if converted == 0 {
    return Err(last_error());
  }
  let descriptor = SecurityDescriptor(descriptor);
  let attributes = SECURITY_ATTRIBUTES {
    nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
    lpSecurityDescriptor: descriptor.0,
    bInheritHandle: 0,
  };
  // SAFETY: `path`, `attributes`, and its descriptor remain valid throughout
  // this synchronous call; the function does not retain their addresses.
  if unsafe { CreateDirectoryW(path.as_ptr(), &attributes) } == 0 {
    return Err(last_error());
  }
  Ok(())
}

pub(super) fn validate_private_access(path: &Path) -> std::io::Result<()> {
  let (owner, dacl, _descriptor) = security(path)?;
  validate_acl(owner, dacl)
}

pub(super) fn validate_trusted_owner(path: &Path) -> std::io::Result<()> {
  let (owner, _, _descriptor) = security(path)?;
  if trusted_object_owner(owner)? {
    Ok(())
  } else {
    Err(untrusted_owner_error("path", owner))
  }
}

pub(super) fn validate_trusted_directory_chain(path: &Path) -> std::io::Result<()> {
  for directory in path.ancestors() {
    if !fs::symlink_metadata(directory)?.is_dir() {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "trusted path chain contains a non-directory",
      ));
    }
    let (owner, dacl, _descriptor) = security(directory)?;
    if !trusted_object_owner(owner)? {
      return Err(untrusted_owner_error("path chain contains a directory", owner));
    }
    validate_ancestor_acl(owner, dacl)
      .map_err(|error| std::io::Error::new(error.kind(), format!("directory '{}': {error}", directory.display())))?;
  }
  Ok(())
}

fn security(path: &Path) -> std::io::Result<(PSID, *mut ACL, SecurityDescriptor)> {
  let path = wide(path)?;
  let mut owner: PSID = ptr::null_mut();
  let mut dacl: *mut ACL = ptr::null_mut();
  let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
  // SAFETY: output pointers are valid and the returned descriptor owns the
  // owner and DACL pointers until the guard is dropped.
  let status = unsafe {
    GetNamedSecurityInfoW(
      path.as_ptr(),
      SE_FILE_OBJECT,
      OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
      &mut owner,
      ptr::null_mut(),
      &mut dacl,
      ptr::null_mut(),
      &mut descriptor,
    )
  };
  if status != 0 {
    return Err(std::io::Error::from_raw_os_error(status as i32));
  }
  let _descriptor = SecurityDescriptor(descriptor);
  if owner.is_null() || dacl.is_null() {
    return Err(private_access_error("path has no owner or has an unrestricted DACL"));
  }
  Ok((owner, dacl, _descriptor))
}

fn validate_acl(owner: PSID, dacl: *mut ACL) -> std::io::Result<()> {
  for_each_allowed_ace(dacl, |mask, _flags, sid| {
    let _ = mask;
    if trusted_sid(sid, owner) {
      Ok(())
    } else {
      Err(private_access_error(
        "path grants access to an untrusted local principal",
      ))
    }
  })
}

fn validate_ancestor_acl(owner: PSID, dacl: *mut ACL) -> std::io::Result<()> {
  for_each_allowed_ace(dacl, |mask, flags, sid| {
    // An inherit-only ACE does not grant rights on this directory. If it is
    // inherited by a child and becomes applicable there, that child's own
    // iteration catches it. The protected credential directory does not
    // inherit untrusted grants at all.
    if ace_grants_replacement(mask, flags) && !trusted_sid(sid, owner) {
      Err(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!(
          "path chain grants replacement rights to an untrusted local principal (mask 0x{mask:08x}, flags 0x{flags:02x})"
        ),
      ))
    } else {
      Ok(())
    }
  })
}

fn ace_grants_replacement(mask: u32, flags: u8) -> bool {
  u32::from(flags) & INHERIT_ONLY_ACE == 0 && mask & ANCESTOR_REPLACEMENT_RIGHTS != 0
}

fn for_each_allowed_ace(
  dacl: *mut ACL,
  mut inspect: impl FnMut(u32, u8, PSID) -> std::io::Result<()>,
) -> std::io::Result<()> {
  let mut information = ACL_SIZE_INFORMATION::default();
  // SAFETY: `dacl` points inside the live descriptor and the destination has
  // the exact size required by `AclSizeInformation`.
  if unsafe {
    GetAclInformation(
      dacl,
      (&mut information as *mut ACL_SIZE_INFORMATION).cast(),
      size_of::<ACL_SIZE_INFORMATION>() as u32,
      AclSizeInformation,
    )
  } == 0
  {
    return Err(last_error());
  }
  for index in 0..information.AceCount {
    let mut raw_ace: *mut c_void = ptr::null_mut();
    // SAFETY: the ACL-reported index is in range and `raw_ace` is checked
    // before dereferencing it as the basic allowed-ACE layout.
    if unsafe { GetAce(dacl, index, &mut raw_ace) } == 0 {
      return Err(last_error());
    }
    if raw_ace.is_null() {
      return Err(private_access_error("path contains an invalid access-control entry"));
    }
    // Every ACE layout starts with ACE_HEADER.
    let ace_type = unsafe { *raw_ace.cast::<u8>() } as u32;
    if is_complex_allowed_ace(ace_type) {
      return Err(private_access_error(
        "path grants access through an unsupported object or callback ACE",
      ));
    }
    if ace_type != ACCESS_ALLOWED_ACE_TYPE {
      continue;
    }
    let ace = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
    // SAFETY: ACCESS_ALLOWED_ACE stores its variable-length SID starting at
    // SidStart; Windows validated the ACE while returning the ACL.
    let sid = unsafe { ptr::addr_of!((*ace).SidStart).cast_mut().cast::<c_void>() };
    // SAFETY: the fixed mask precedes the variable-length SID in the validated
    // ACCESS_ALLOWED_ACE returned by Windows.
    inspect(unsafe { (*ace).Mask }, unsafe { (*ace).Header.AceFlags }, sid)?;
  }
  Ok(())
}

fn trusted_object_owner(owner: PSID) -> std::io::Result<bool> {
  // SAFETY: well-known SID inspection reads the valid SID owned by the live
  // security descriptor held by the caller.
  if unsafe { IsWellKnownSid(owner, WinLocalSystemSid) != 0 || IsWellKnownSid(owner, WinBuiltinAdministratorsSid) != 0 }
  {
    return Ok(true);
  }
  if is_trusted_installer(owner)? {
    return Ok(true);
  }
  let mut token = ptr::null_mut();
  // SAFETY: the pseudo process handle is valid for the duration of the call
  // and `token` receives one owned handle on success.
  if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
    return Err(last_error());
  }
  let token = Handle(token);
  let mut required = 0_u32;
  // SAFETY: a null buffer with length zero is the documented sizing call.
  unsafe {
    GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required);
  }
  if required == 0 {
    return Err(last_error());
  }
  let words = (required as usize).div_ceil(size_of::<usize>());
  let mut buffer = vec![0_usize; words];
  // SAFETY: the aligned allocation has at least `required` writable bytes and
  // remains live while the returned TOKEN_USER SID is compared.
  if unsafe { GetTokenInformation(token.0, TokenUser, buffer.as_mut_ptr().cast(), required, &mut required) } == 0 {
    return Err(last_error());
  }
  let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
  // SAFETY: both SIDs remain valid for this comparison.
  Ok(unsafe { EqualSid(owner, user.User.Sid) != 0 })
}

pub(super) fn open_regular_file_no_follow(path: &Path) -> std::io::Result<fs::File> {
  let file = fs::OpenOptions::new()
    .read(true)
    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
    .open(path)?;
  let metadata = file.metadata()?;
  if !metadata.file_type().is_file() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidInput,
      "path is not a regular non-reparse file",
    ));
  }
  Ok(file)
}

pub(super) fn is_reparse_point(path: &Path) -> std::io::Result<bool> {
  Ok(fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

pub(super) fn file_identity(file: &fs::File) -> std::io::Result<FileIdentity> {
  let mut information = FILE_ID_INFO::default();
  // SAFETY: the raw handle remains owned by `file`; `information` has the
  // exact layout and writable size required by FileIdInfo for this call.
  if unsafe {
    GetFileInformationByHandleEx(
      file.as_raw_handle() as HANDLE,
      FileIdInfo,
      (&mut information as *mut FILE_ID_INFO).cast(),
      size_of::<FILE_ID_INFO>() as u32,
    )
  } == 0
  {
    return Err(last_error());
  }
  Ok(FileIdentity {
    volume_serial: information.VolumeSerialNumber,
    file_id: information.FileId.Identifier,
  })
}

fn trusted_sid(sid: PSID, owner: PSID) -> bool {
  // SAFETY: both SID pointers come from the live security descriptor and are
  // consumed only by Windows SID inspection functions.
  unsafe {
    EqualSid(sid, owner) != 0
      || IsWellKnownSid(sid, WinLocalSystemSid) != 0
      || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
      || IsWellKnownSid(sid, WinCreatorOwnerRightsSid) != 0
      || IsWellKnownSid(sid, WinCreatorOwnerSid) != 0
  }
}

fn is_trusted_installer(sid: PSID) -> std::io::Result<bool> {
  Ok(sid_string(sid)? == TRUSTED_INSTALLER_SID)
}

fn sid_string(sid: PSID) -> std::io::Result<String> {
  let mut value = ptr::null_mut();
  // SAFETY: `sid` points into a live security descriptor. Windows allocates a
  // NUL-terminated string and transfers it to the returned local-memory guard.
  if unsafe { ConvertSidToStringSidW(sid, &mut value) } == 0 {
    return Err(last_error());
  }
  let value = LocalWideString(value);
  let mut length = 0;
  // SAFETY: ConvertSidToStringSidW guarantees a NUL-terminated UTF-16 string
  // which remains live until `value` is dropped below.
  while unsafe { *value.0.add(length) } != 0 {
    length += 1;
  }
  // SAFETY: the preceding scan found the terminator inside the Windows-owned
  // allocation, so these `length` code units are initialized and readable.
  String::from_utf16(unsafe { std::slice::from_raw_parts(value.0, length) })
    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn untrusted_owner_error(context: &str, owner: PSID) -> std::io::Error {
  let owner = sid_string(owner).unwrap_or_else(|error| format!("unavailable SID: {error}"));
  std::io::Error::new(
    std::io::ErrorKind::PermissionDenied,
    format!("{context} has untrusted owner {owner}"),
  )
}

fn is_complex_allowed_ace(ace_type: u32) -> bool {
  matches!(
    ace_type,
    ACCESS_ALLOWED_OBJECT_ACE_TYPE
      | ACCESS_ALLOWED_CALLBACK_ACE_TYPE
      | ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
      | ACCESS_ALLOWED_COMPOUND_ACE_TYPE
  )
}

fn wide(path: &Path) -> std::io::Result<Vec<u16>> {
  let value = path.as_os_str().encode_wide().collect::<Vec<_>>();
  if value.contains(&0) {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidInput,
      "path contains a NUL character",
    ));
  }
  Ok(value.into_iter().chain(Some(0)).collect())
}

fn last_error() -> std::io::Error {
  // SAFETY: GetLastError has no preconditions and is read immediately after
  // the failing Windows API call on the same thread.
  std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32)
}

fn private_access_error(message: &'static str) -> std::io::Error {
  std::io::Error::new(std::io::ErrorKind::PermissionDenied, message)
}

struct SecurityDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for SecurityDescriptor {
  fn drop(&mut self) {
    if !self.0.is_null() {
      // SAFETY: both Windows APIs used here return a LocalAlloc-owned
      // descriptor and ownership is transferred exactly once to this guard.
      unsafe {
        LocalFree(self.0);
      }
    }
  }
}

struct LocalWideString(*mut u16);

impl Drop for LocalWideString {
  fn drop(&mut self) {
    if !self.0.is_null() {
      // SAFETY: ConvertSidToStringSidW returned this LocalAlloc-owned buffer
      // and ownership is released exactly once by this guard.
      unsafe {
        LocalFree(self.0.cast());
      }
    }
  }
}

struct Handle(HANDLE);

impl Drop for Handle {
  fn drop(&mut self) {
    if !self.0.is_null() {
      // SAFETY: OpenProcessToken returned this owned handle and the guard
      // closes it exactly once.
      unsafe {
        CloseHandle(self.0);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn distinguishes_sibling_creation_from_path_replacement() {
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_ADD_FILE: u32 = 0x0000_0002;
    const FILE_ADD_SUBDIRECTORY: u32 = 0x0000_0004;

    assert!(!ace_grants_replacement(
      GENERIC_WRITE | FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY,
      0
    ));
    assert!(ace_grants_replacement(FILE_DELETE_CHILD, 0));
    assert!(ace_grants_replacement(DELETE, 0));
  }

  #[test]
  fn ignores_replacement_rights_which_do_not_apply_to_the_directory() {
    assert!(!ace_grants_replacement(FILE_DELETE_CHILD, INHERIT_ONLY_ACE as u8));
  }

  #[test]
  fn rejects_an_untrusted_delete_child_grant_on_an_ancestor() {
    let temporary = tempfile::tempdir().unwrap();
    let directory = temporary.path().join("replaceable");
    fs::create_dir(&directory).unwrap();
    // `WD` is Everyone. SDDL has no mnemonic for the filesystem-specific
    // FILE_DELETE_CHILD right; its `DC` token means the unrelated Active
    // Directory mask 0x2. Use the exact filesystem mask so this descriptor
    // models an ancestor able to replace a protected descendant.
    let sddl = "D:P(A;;0x40;;;WD)".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
    // SAFETY: the SDDL input is NUL-terminated and the returned descriptor is
    // owned by the guard through the SetFileSecurityW call.
    assert_ne!(
      unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
          sddl.as_ptr(),
          SDDL_REVISION_1,
          &mut descriptor,
          ptr::null_mut(),
        )
      },
      0
    );
    let descriptor = SecurityDescriptor(descriptor);
    let path = wide(&directory).unwrap();
    // SAFETY: both the path and descriptor remain live for the synchronous
    // call and the descriptor contains a valid DACL produced by Windows.
    assert_ne!(
      unsafe { windows_sys::Win32::Security::SetFileSecurityW(path.as_ptr(), DACL_SECURITY_INFORMATION, descriptor.0) },
      0
    );

    let (owner, dacl, _descriptor) = security(&directory).unwrap();
    assert!(validate_ancestor_acl(owner, dacl).is_err());
  }
}
