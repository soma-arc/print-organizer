#pragma once

#include <memory>
#include <string>
#include <vector>

#include <openvdb/openvdb.h>

#include "genmesh/bricks_data.h"
#include "genmesh/exit_code.h"
#include "genmesh/manifest.h"

namespace genmesh {

/// Result of VDB grid construction.
struct VdbBuildResult {
    openvdb::FloatGrid::Ptr grid;
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::string error_code;
    std::string error_msg;
    int64_t active_voxel_count = 0;
};

/// Initialize OpenVDB. Must be called once before any VDB operations.
/// Returns false on failure.
bool vdb_init();

/// Create an empty FloatGrid with the correct Transform and background value.
///
/// - Transform: linear with voxel_size, translated by aabb_min.
/// - Background: manifest.background_value_mm
/// - Grid class: GRID_LEVEL_SET
openvdb::FloatGrid::Ptr create_grid(const Manifest& manifest);

/// Build a VDB FloatGrid from brick data.
///
/// 1. Creates grid via create_grid().
/// 2. Iterates over bricks and sets voxel values.
/// 3. Bricks not present in the data are left as background (sparse convention §5.5).
VdbBuildResult build_vdb(const Manifest& manifest,
                         const std::vector<BrickData>& bricks);

}  // namespace genmesh
