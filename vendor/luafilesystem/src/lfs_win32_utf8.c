#ifdef _WIN32

#include "lfs_win32_utf8.h"

#include <direct.h>
#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

static void lfs_set_errno_from_win32(DWORD error)
{
  switch (error) {
  case ERROR_FILE_NOT_FOUND:
  case ERROR_PATH_NOT_FOUND:
  case ERROR_INVALID_DRIVE:
    errno = ENOENT;
    break;
  case ERROR_ACCESS_DENIED:
  case ERROR_SHARING_VIOLATION:
  case ERROR_LOCK_VIOLATION:
    errno = EACCES;
    break;
  case ERROR_ALREADY_EXISTS:
  case ERROR_FILE_EXISTS:
    errno = EEXIST;
    break;
  case ERROR_FILENAME_EXCED_RANGE:
    errno = ENAMETOOLONG;
    break;
  case ERROR_NOT_ENOUGH_MEMORY:
  case ERROR_OUTOFMEMORY:
    errno = ENOMEM;
    break;
  default:
    errno = EINVAL;
    break;
  }
}

wchar_t *lfs_utf8_to_utf16(const char *utf8)
{
  int length;
  wchar_t *wide;
  if (utf8 == NULL) {
    errno = EINVAL;
    return NULL;
  }
  length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, -1, NULL, 0);
  if (length <= 0) {
    lfs_set_errno_from_win32(GetLastError());
    return NULL;
  }
  wide = (wchar_t *)malloc((size_t)length * sizeof(wchar_t));
  if (wide == NULL) {
    errno = ENOMEM;
    return NULL;
  }
  if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8, -1, wide, length) <= 0) {
    lfs_set_errno_from_win32(GetLastError());
    free(wide);
    return NULL;
  }
  return wide;
}

char *lfs_utf16_to_utf8(const wchar_t *wide)
{
  int length;
  char *utf8;
  if (wide == NULL) {
    errno = EINVAL;
    return NULL;
  }
  length = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, NULL, 0, NULL, NULL);
  if (length <= 0) {
    lfs_set_errno_from_win32(GetLastError());
    return NULL;
  }
  utf8 = (char *)malloc((size_t)length);
  if (utf8 == NULL) {
    errno = ENOMEM;
    return NULL;
  }
  if (WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, wide, -1, utf8, length, NULL, NULL) <= 0) {
    lfs_set_errno_from_win32(GetLastError());
    free(utf8);
    return NULL;
  }
  return utf8;
}

wchar_t *lfs_utf8_to_path(const char *utf8)
{
  wchar_t *input = lfs_utf8_to_utf16(utf8);
  wchar_t *absolute;
  wchar_t *extended;
  DWORD length;
  size_t absolute_length;
  size_t prefix_length;
  int unc;
  if (input == NULL)
    return NULL;

  length = GetFullPathNameW(input, 0, NULL, NULL);
  if (length == 0) {
    lfs_set_errno_from_win32(GetLastError());
    free(input);
    return NULL;
  }
  absolute = (wchar_t *)malloc(((size_t)length + 1) * sizeof(wchar_t));
  if (absolute == NULL) {
    free(input);
    errno = ENOMEM;
    return NULL;
  }
  if (GetFullPathNameW(input, length + 1, absolute, NULL) == 0) {
    lfs_set_errno_from_win32(GetLastError());
    free(absolute);
    free(input);
    return NULL;
  }
  free(input);

  if (wcsncmp(absolute, L"\\\\?\\", 4) == 0)
    return absolute;

  unc = wcsncmp(absolute, L"\\\\", 2) == 0;
  absolute_length = wcslen(absolute);
  prefix_length = unc ? 8 : 4;
  extended = (wchar_t *)malloc((prefix_length + absolute_length + 1) * sizeof(wchar_t));
  if (extended == NULL) {
    free(absolute);
    errno = ENOMEM;
    return NULL;
  }
  if (unc) {
    wcscpy(extended, L"\\\\?\\UNC\\");
    wcscat(extended, absolute + 2);
  } else {
    wcscpy(extended, L"\\\\?\\");
    wcscat(extended, absolute);
  }
  free(absolute);
  return extended;
}

int lfs_win32_chdir(const char *path)
{
  wchar_t *wide = lfs_utf8_to_path(path);
  int result;
  if (wide == NULL)
    return -1;
  result = SetCurrentDirectoryW(wide) ? 0 : -1;
  if (result != 0)
    lfs_set_errno_from_win32(GetLastError());
  free(wide);
  return result;
}

char *lfs_win32_getcwd(void)
{
  DWORD length = GetCurrentDirectoryW(0, NULL);
  wchar_t *wide;
  char *utf8;
  if (length == 0) {
    lfs_set_errno_from_win32(GetLastError());
    return NULL;
  }
  wide = (wchar_t *)malloc(((size_t)length + 1) * sizeof(wchar_t));
  if (wide == NULL) {
    errno = ENOMEM;
    return NULL;
  }
  if (GetCurrentDirectoryW(length + 1, wide) == 0) {
    lfs_set_errno_from_win32(GetLastError());
    free(wide);
    return NULL;
  }
  utf8 = lfs_utf16_to_utf8(wide);
  free(wide);
  return utf8;
}

int lfs_win32_mkdir(const char *path)
{
  wchar_t *wide = lfs_utf8_to_path(path);
  int result;
  if (wide == NULL)
    return -1;
  result = CreateDirectoryW(wide, NULL) ? 0 : -1;
  if (result != 0)
    lfs_set_errno_from_win32(GetLastError());
  free(wide);
  return result;
}

int lfs_win32_rmdir(const char *path)
{
  wchar_t *wide = lfs_utf8_to_path(path);
  int result;
  if (wide == NULL)
    return -1;
  result = RemoveDirectoryW(wide) ? 0 : -1;
  if (result != 0)
    lfs_set_errno_from_win32(GetLastError());
  free(wide);
  return result;
}

int lfs_win32_stat(const char *path, struct _stati64 *buffer)
{
  wchar_t *wide = lfs_utf8_to_path(path);
  int result;
  if (wide == NULL)
    return -1;
  result = _wstati64(wide, buffer);
  free(wide);
  return result;
}

int lfs_win32_utime(const char *path, const struct utimbuf *times)
{
  wchar_t *wide = lfs_utf8_to_path(path);
  struct __utimbuf64 wide_times;
  int result;
  if (wide == NULL)
    return -1;
  if (times != NULL) {
    wide_times.actime = (__time64_t)times->actime;
    wide_times.modtime = (__time64_t)times->modtime;
    result = _wutime64(wide, &wide_times);
  } else {
    result = _wutime64(wide, NULL);
  }
  free(wide);
  return result;
}

#endif
