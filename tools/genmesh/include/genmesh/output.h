#pragma once

#include <string>
#include <vector>

#include "genmesh/exit_code.h"

namespace genmesh {

/// Result of output directory preparation.
struct OutputDirResult {
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::string error_code;   // GENMESH_E* if !ok
    std::string error_msg;
};

/// Prepare the output directory for writing.
///
/// - Creates `out_dir` (including parents) if it does not exist.
/// - Checks whether expected output files already exist:
///     mesh.stl  (if write_stl)
///     volume.vdb (if write_vdb)
///     report.json (always)
/// - If any exist and `force` is false → IoError + E2005.
/// - If any exist and `force` is true  → OK (will overwrite later).
///
/// Returns OutputDirResult.
OutputDirResult prepare_output_dir(const std::string& out_dir,
                                   bool write_stl,
                                   bool write_vdb,
                                   bool force);

}  // namespace genmesh
