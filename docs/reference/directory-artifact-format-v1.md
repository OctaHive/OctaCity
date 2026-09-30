# Directory artifact format v1

OctaCity uploads a declared artifact directory as an uncompressed POSIX tar
stream with media type `application/vnd.octacity.directory.tar.v1`.

The agent walks entries without following symbolic links and writes a stable
USTAR archive. The byte-level framing is part of version 1:

- every header is a 512-byte POSIX USTAR header with `ustar\0` magic and
  `00` version; GNU and PAX extension records are never emitted;
- names must fit the USTAR `name`/`prefix` fields and link targets must fit the
  USTAR `linkname` field; an unrepresentable value rejects the output instead
  of silently choosing a host-dependent extension;
- numeric header fields use the USTAR octal representation and the checksum is
  computed after all normalized fields are written;
- each regular-file body is followed by zero bytes up to the next 512-byte
  record, and the stream ends with two all-zero records;
- no additional block-size padding is appended after the two terminal records.

Within that framing:

- paths and symbolic-link targets use UTF-8 `/` components and are relative to
  the declared directory; names containing controls, `\\`, or `:` are rejected
  so one tree has the same representation on every supported host;
- entries are emitted in depth-first lexical path order, with a directory
  immediately before its children;
- user and group IDs, user and group names, and modification times are zero;
- directory modes are `0755`, regular files are `0644` or `0755` according to
  their executable bit, and symbolic-link modes are `0777`;
- file bytes are streamed into the archive and the source metadata is checked
  before and after the read;
- an absolute symbolic link or a link resolving outside the declared artifact
  directory rejects the complete output set;
- devices, sockets, FIFOs, non-UTF-8 names, traversal, and entries beyond the
  configured bound reject the complete output set.

The SHA-256 and byte size sent to the coordinator describe the complete tar
stream, not the source directory contents. Changing any path, normalized mode,
link target, or file byte changes the uploaded object digest.
