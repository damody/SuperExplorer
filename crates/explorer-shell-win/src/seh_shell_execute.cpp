#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <shellapi.h>

// ShellExecuteW for a Visual Studio solution can load an in-process handler that
// throws a C++ exception. If that exception reaches Rust frames it aborts the
// process while dropping GPUI jump-list paths. Catch it at this boundary.
static int shell_execute_cxx(const wchar_t* path, const wchar_t* directory) {
    try {
        const HINSTANCE result = ShellExecuteW(
            nullptr,
            nullptr,
            path,
            nullptr,
            directory,
            SW_SHOWNORMAL);
        return static_cast<int>(reinterpret_cast<INT_PTR>(result));
    } catch (...) {
        return -1;
    }
}

extern "C" int seh_shell_execute_w(const wchar_t* path, const wchar_t* directory) {
    if (path == nullptr) {
        return 0;
    }
    // Structured and C++ handlers cannot share one function. The outer frame
    // catches SEH; the inner frame catches C++ exceptions from the association.
    __try {
        return shell_execute_cxx(path, directory);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        return static_cast<int>(GetExceptionCode());
    }
}
