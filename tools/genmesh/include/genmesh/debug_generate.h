#pragma once

#include <string>
#include <vector>

#include "genmesh/bricks_data.h"
#include "genmesh/exit_code.h"
#include "genmesh/manifest.h"

namespace genmesh {

/// Result of debug SDF generation.
struct DebugGenerateResult {
    Manifest manifest;
    std::vector<BrickData> bricks;
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::string error_msg;
};

/// Generate a debug SDF entirely inside the CLI.
///
/// @param shape  "sphere" or "box"
/// @param dims   Grid dimensions (e.g. 64). Grid is dims x dims x dims.
/// @param voxel_size  Voxel size in mm.
///
/// For "sphere": centered SDF sphere of radius = dims*voxel_size*0.4.
/// For "box":    centered SDF box of half-extents = dims*voxel_size*0.3 per axis.
///
/// The manifest is fully valid with fixed coordinate_system values.
/// Data is split into B^3 bricks matching manifest.brick_size.
DebugGenerateResult debug_generate(const std::string& shape,
                                   int dims = 64,
                                   float voxel_size = 1.0f);

}  // namespace genmesh
