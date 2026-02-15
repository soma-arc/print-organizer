#pragma once

#include <iostream>
#include <sstream>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace genmesh {

/// Log levels per spec section 9.2
enum class LogLevel : int {
    Error = 0,
    Warn  = 1,
    Info  = 2,
    Debug = 3,
};

/// Parse log level from string. Returns Info on unrecognized input.
inline LogLevel parse_log_level(std::string_view s) {
    if (s == "error") return LogLevel::Error;
    if (s == "warn")  return LogLevel::Warn;
    if (s == "info")  return LogLevel::Info;
    if (s == "debug") return LogLevel::Debug;
    return LogLevel::Info;
}

inline std::string_view log_level_str(LogLevel lv) {
    switch (lv) {
        case LogLevel::Error: return "ERROR";
        case LogLevel::Warn:  return "WARN";
        case LogLevel::Info:  return "INFO";
        case LogLevel::Debug: return "DEBUG";
    }
    return "INFO";
}

/// Key-value context pair for structured log output
using KV = std::pair<std::string, std::string>;

/// Global minimum log level (set from --log-level)
inline LogLevel& min_log_level() {
    static LogLevel level = LogLevel::Info;
    return level;
}

/// Structured stderr log output per spec section 9.2.
///
/// Format: LEVEL CODE: message | key=value key=value ...
/// Example: ERROR GENMESH_E2001: manifest missing field | field=dims path=project.json
inline void log(LogLevel level, std::string_view code, std::string_view message,
                const std::vector<KV>& context = {}) {
    if (static_cast<int>(level) > static_cast<int>(min_log_level())) {
        return;
    }

    std::ostringstream out;
    out << log_level_str(level) << " " << code << ": " << message;

    if (!context.empty()) {
        out << " |";
        for (const auto& [k, v] : context) {
            out << " " << k << "=" << v;
        }
    }

    std::cerr << out.str() << "\n";
}

// Convenience wrappers
inline void log_error(std::string_view code, std::string_view message,
                      const std::vector<KV>& ctx = {}) {
    log(LogLevel::Error, code, message, ctx);
}

inline void log_warn(std::string_view code, std::string_view message,
                     const std::vector<KV>& ctx = {}) {
    log(LogLevel::Warn, code, message, ctx);
}

inline void log_info(std::string_view code, std::string_view message,
                     const std::vector<KV>& ctx = {}) {
    log(LogLevel::Info, code, message, ctx);
}

inline void log_debug(std::string_view code, std::string_view message,
                      const std::vector<KV>& ctx = {}) {
    log(LogLevel::Debug, code, message, ctx);
}

}  // namespace genmesh
