#include "genmesh/cli.h"
#include "genmesh/exit_code.h"

#include <iostream>
#include <string>
#include <string_view>
#include <cstring>

namespace genmesh {

void print_usage() {
    std::cerr <<
R"(Usage: genmesh --manifest <path> --in <path> --out <dir> [options]

Required:
  --manifest <path>       Path to manifest (project.json)
  --in <path>             Input directory containing bricks.bin + bricks.index.json
  --out <dir>             Output directory (created if missing)

Options:
  --write-stl             Write mesh.stl (default: true)
  --no-write-stl          Disable STL output
  --write-vdb             Write volume.vdb (default: false)
  --iso <float>           Iso-surface value (default: manifest.iso or 0.0)
  --adaptivity <float>    Mesh adaptivity 0.0-1.0 (default: manifest.adaptivity or 0.0)
  --force                 Overwrite existing output files
  --log-level <level>     error|warn|info|debug (default: info)
  --debug-generate <shape> Generate test distance field: sphere|box
  --help                  Show this help
)";
}

// helper: check next arg exists
static bool need_value(int i, int argc, const char* flag, ParseResult& result) {
    if (i + 1 >= argc) {
        result.ok = false;
        result.exit_code = static_cast<int>(ExitCode::General);
        result.error_msg = std::string("Missing value for ") + flag;
        return false;
    }
    return true;
}

ParseResult parse_args(int argc, char* argv[]) {
    ParseResult result;
    result.ok = true;

    if (argc <= 1) {
        result.args.help = true;
        result.ok = true;
        return result;
    }

    bool has_manifest = false;
    bool has_in = false;
    bool has_out = false;
    bool explicit_write_stl = false;

    for (int i = 1; i < argc; ++i) {
        std::string_view arg = argv[i];

        if (arg == "--help" || arg == "-h") {
            result.args.help = true;
            result.ok = true;
            return result;
        }
        else if (arg == "--manifest") {
            if (!need_value(i, argc, "--manifest", result)) return result;
            result.args.manifest_path = argv[++i];
            has_manifest = true;
        }
        else if (arg == "--in") {
            if (!need_value(i, argc, "--in", result)) return result;
            result.args.in_dir = argv[++i];
            has_in = true;
        }
        else if (arg == "--out") {
            if (!need_value(i, argc, "--out", result)) return result;
            result.args.out_dir = argv[++i];
            has_out = true;
        }
        else if (arg == "--write-stl") {
            result.args.write_stl = true;
            explicit_write_stl = true;
        }
        else if (arg == "--no-write-stl") {
            result.args.write_stl = false;
            explicit_write_stl = true;
        }
        else if (arg == "--write-vdb") {
            result.args.write_vdb = true;
        }
        else if (arg == "--iso") {
            if (!need_value(i, argc, "--iso", result)) return result;
            try {
                result.args.iso = std::stof(argv[++i]);
            } catch (...) {
                result.ok = false;
                result.exit_code = static_cast<int>(ExitCode::General);
                result.error_msg = "Invalid value for --iso";
                return result;
            }
        }
        else if (arg == "--adaptivity") {
            if (!need_value(i, argc, "--adaptivity", result)) return result;
            try {
                result.args.adaptivity = std::stof(argv[++i]);
            } catch (...) {
                result.ok = false;
                result.exit_code = static_cast<int>(ExitCode::General);
                result.error_msg = "Invalid value for --adaptivity";
                return result;
            }
        }
        else if (arg == "--force") {
            result.args.force = true;
        }
        else if (arg == "--log-level") {
            if (!need_value(i, argc, "--log-level", result)) return result;
            std::string val = argv[++i];
            if (val != "error" && val != "warn" && val != "info" && val != "debug") {
                result.ok = false;
                result.exit_code = static_cast<int>(ExitCode::General);
                result.error_msg = "Invalid log level: " + val;
                return result;
            }
            result.args.log_level = val;
        }
        else if (arg == "--debug-generate") {
            if (!need_value(i, argc, "--debug-generate", result)) return result;
            std::string val = argv[++i];
            if (val != "sphere" && val != "box") {
                result.ok = false;
                result.exit_code = static_cast<int>(ExitCode::General);
                result.error_msg = "Invalid debug shape: " + val + " (expected sphere|box)";
                return result;
            }
            result.args.debug_generate = val;
        }
        else {
            result.ok = false;
            result.exit_code = static_cast<int>(ExitCode::General);
            result.error_msg = std::string("Unknown argument: ") + std::string(arg);
            return result;
        }
    }

    // --debug-generate relaxes required args (manifest/in not needed)
    if (!result.args.debug_generate.empty()) {
        if (!has_out) {
            result.ok = false;
            result.exit_code = static_cast<int>(ExitCode::ValidationFailure);
            result.error_msg = "Missing required argument: --out";
            return result;
        }
        return result;
    }

    // Validate required args
    if (!has_manifest || !has_in || !has_out) {
        result.ok = false;
        result.exit_code = static_cast<int>(ExitCode::ValidationFailure);
        std::string missing;
        if (!has_manifest) missing += " --manifest";
        if (!has_in)       missing += " --in";
        if (!has_out)      missing += " --out";
        result.error_msg = "Missing required argument(s):" + missing;
        return result;
    }

    return result;
}

}  // namespace genmesh
