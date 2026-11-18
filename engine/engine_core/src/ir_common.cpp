#include "ir_common.h"

namespace ir {

static std::mutex g_error_mutex;
static std::string g_error;

void set_last_error(const std::string& message) {
    std::lock_guard<std::mutex> lock(g_error_mutex);
    g_error = message;
}
std::string last_error() {
    std::lock_guard<std::mutex> lock(g_error_mutex);
    return g_error;
}

std::string wide_to_utf8(const wchar_t* s, size_t len) {
    if (!s || len == 0) return {};
    int n = WideCharToMultiByte(CP_UTF8, 0, s, (int)len, nullptr, 0, nullptr, nullptr);
    std::string out(n > 0 ? n : 0, '\0');
    if (n > 0) WideCharToMultiByte(CP_UTF8, 0, s, (int)len, out.data(), n, nullptr, nullptr);
    return out;
}
std::wstring utf8_to_wide(const std::string& s) {
    if (s.empty()) return {};
    int n = MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), nullptr, 0);
    std::wstring out(n > 0 ? n : 0, L'\0');
    if (n > 0) MultiByteToWideChar(CP_UTF8, 0, s.data(), (int)s.size(), out.data(), n);
    return out;
}

} // namespace ir
