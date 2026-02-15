#include "genmesh/output.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <filesystem>
#include <system_error>
#include <vector>
#include <string>

namespace fs = std::filesystem;

namespace genmesh {

OutputDirResult prepare_output_dir(const std::string& out_dir,
                                   bool write_stl,
                                   bool write_vdb,
                                   bool force) {
    OutputDirResult result;

    // --- create directory (mkdir -p) ---
    std::error_code ec;
    fs::create_directories(out_dir, ec);
    if (ec) {
        result.ok = false;
        result.exit_code = ExitCode::IoError;
        result.error_code = std::string(E2004);
        result.error_msg = "Cannot create output directory: " + out_dir + " (" + ec.message() + ")";
        log_error(E2004, result.error_msg);
        return result;
    }

    // Verify the path is actually a directory
    if (!fs::is_directory(out_dir, ec)) {
        result.ok = false;
        result.exit_code = ExitCode::IoError;
        result.error_code = std::string(E2004);
        result.error_msg = "Output path exists but is not a directory: " + out_dir;
        log_error(E2004, result.error_msg);
        return result;
    }

    // --- check for existing output files ---
    std::vector<std::string> filenames;
    filenames.push_back("report.json");
    if (write_stl) filenames.push_back("mesh.stl");
    if (write_vdb) filenames.push_back("volume.vdb");

    for (const auto& name : filenames) {
        fs::path p = fs::path(out_dir) / name;
        if (fs::exists(p, ec)) {
            if (!force) {
                result.ok = false;
                result.exit_code = ExitCode::IoError;
                result.error_code = std::string(E2005);
                result.error_msg = "Output file already exists: " + p.string() +
                                   " (use --force to overwrite)";
                log_error(E2005, result.error_msg, {{"path", p.string()}});
                return result;
            }
            // force=true: log warning, continue
            log_warn(std::string_view("GENMESH_W2001"),
                     "Will overwrite existing file: " + p.string(),
                     {{"path", p.string()}});
        }
    }

    result.ok = true;
    result.exit_code = ExitCode::Success;
    return result;
}

}  // namespace genmesh
