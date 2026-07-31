#ifndef LFS_WIN32_UTF8_H
#define LFS_WIN32_UTF8_H

#ifdef _WIN32

#include <stddef.h>
#include <sys/stat.h>
#include <sys/utime.h>
#include <wchar.h>

wchar_t *lfs_utf8_to_utf16(const char *utf8);
wchar_t *lfs_utf8_to_path(const char *utf8);
char *lfs_utf16_to_utf8(const wchar_t *wide);

int lfs_win32_chdir(const char *path);
char *lfs_win32_getcwd(void);
int lfs_win32_mkdir(const char *path);
int lfs_win32_rmdir(const char *path);
int lfs_win32_stat(const char *path, struct _stati64 *buffer);
int lfs_win32_utime(const char *path, const struct utimbuf *times);

#endif
#endif
