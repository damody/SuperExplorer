[![License](http://img.shields.io/badge/Licence-MIT-brightgreen.svg)](LICENSE)
[![Build Status](https://github.com/lunarmodules/luafilesystem/actions/workflows/ci.yml/badge.svg)](https://github.com/lunarmodules/luafilesystem/actions)
[![Build status](https://ci.appveyor.com/api/projects/status/y04s4ms7u16trw8e?svg=true)](https://ci.appveyor.com/project/ignacio/luafilesystem)
[![Coverage Status](https://coveralls.io/repos/lunarmodules/luafilesystem/badge.png)](https://coveralls.io/r/lunarmodules/luafilesystem)

# LuaFileSystem - File System Library for Lua

https://lunarmodules.github.io/luafilesystem

# Description

LuaFileSystem is a Lua library developed to complement the set of functions
related to file systems offered by the standard Lua distribution.

LuaFileSystem offers a portable way to access the underlying directory structure and file attributes.
LuaFileSystem is free software and uses the same license as Lua 5.x (MIT).

# LuaRocks Installation

```
luarocks install luafilesystem
```

# Documentation

Please check the at `docs/` for more information, also available at the [project website](https://lunarmodules.github.io/luafilesystem).

# Windows UTF-8 paths in this fork

On Windows, path arguments are interpreted as UTF-8 and converted with
`MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, ...)`. Filesystem calls use
the wide Win32 API, and directory entry names are converted back to UTF-8.
This supports non-ASCII names and extended-length paths without changing the
LuaFileSystem API or module name.

The Windows implementation covers `attributes`, `symlinkattributes`, `chdir`,
`currentdir`, `dir`, `link`, `lock_dir`, `mkdir`, `rmdir`, and `touch`. Run
`tests/test_unicode_windows.lua` with a disposable absolute directory to verify
Traditional Chinese, Japanese, Korean, emoji, combining characters, and paths
longer than `MAX_PATH`.
