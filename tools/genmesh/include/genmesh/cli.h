#pragma once

#include <optional>
#include <string>

namespace genmesh {

/// Parsed CLI arguments per spec section 2
struct CliArgs {
    // Required
    std::string manifest_path;
    std::string in_dir;
    std::string out_dir;

    // Optional flags
    bool write_stl   = true;
    bool write_vdb   = false;
    bool force       = false;

    // Optional values (nullopt = use manifest value)
    std::optional<float> iso;
    std::optional<float> adaptivity;

    // Log level string
    std::string log_level = "info";

    // Debug
    std::string debug_generate;  // "" | "sphere" | "box"

    // Help requested
    bool help = false;
};

/// Result of argument parsing
struct ParseResult {
    CliArgs args;
    bool ok = false;
    int exit_code = 0;      // non-zero on error
    std::string error_msg;  // human-readable error message
};

/// Parse command-line arguments.
/// Returns ParseResult with ok=true on success, or ok=false with exit_code set.
ParseResult parse_args(int argc, char* argv[]);

/// Print usage/help to stderr.
void print_usage();

}  // namespace genmesh
