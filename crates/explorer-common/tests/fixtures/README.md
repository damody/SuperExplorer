# Archive browsing fixture

`browse.rar` is the 336-byte `test_read_format_rar.rar` fixture from the libarchive
test suite, decoded from its uuencoded representation:
https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar.rar.uu

The libarchive project is distributed under the BSD license:
https://github.com/libarchive/libarchive/blob/master/COPYING

It contains `test.txt`, `testdir/test.txt`, empty directories and a symbolic link.
`browse-rar5.rar` is the 1,656-byte `test_read_format_rar5_multiple_files.rar`
fixture from the same project:
https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar5_multiple_files.rar.uu

The redistribution license is included as `libarchive-COPYING`.

Tests verify real RAR and RAR5 reading and skip the link. ZIP and 7z fixtures are generated
in private temporary directories during tests.
