#include "genmesh/vdb_builder.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <openvdb/openvdb.h>
#include <openvdb/math/Transform.h>

#include <cmath>
#include <string>

namespace genmesh {

bool vdb_init() {
    try {
        openvdb::initialize();
        return true;
    } catch (const std::exception& e) {
        log_error(E3001, std::string("openvdb::initialize() failed: ") + e.what());
        return false;
    }
}

openvdb::FloatGrid::Ptr create_grid(const Manifest& manifest) {
    // Create linear transform: index-space → world-space
    auto xform = openvdb::math::Transform::createLinearTransform(
        static_cast<double>(manifest.voxel_size));

    // Apply aabb_min as translation so that index (0,0,0) maps to aabb_min + 0.5*voxel_size
    // OpenVDB convention: worldPos = voxelSize * (indexPos + 0.5) when using voxel-center.
    // But createLinearTransform already handles this via the half-voxel offset in its
    // indexToWorld mapping. We need to shift the origin so that:
    //   worldPos(0,0,0) = aabb_min + 0.5 * voxel_size
    // Default: worldPos(0,0,0) = 0.5 * voxel_size  (with center-voxel convention)
    // So we translate by aabb_min.
    xform->postTranslate(openvdb::math::Vec3d(
        manifest.aabb_min[0],
        manifest.aabb_min[1],
        manifest.aabb_min[2]));

    // Create grid with background value
    auto grid = openvdb::FloatGrid::create(manifest.background_value_mm);
    grid->setTransform(xform);
    grid->setGridClass(openvdb::GRID_LEVEL_SET);
    grid->setName("distance");

    return grid;
}

VdbBuildResult build_vdb(const Manifest& manifest,
                         const std::vector<BrickData>& bricks) {
    VdbBuildResult result;

    // Create grid
    try {
        result.grid = create_grid(manifest);
    } catch (const std::exception& e) {
        result.ok = false;
        result.exit_code = ExitCode::ProcessingError;
        result.error_code = std::string(E4001);
        result.error_msg = std::string("Grid creation failed: ") + e.what();
        log_error(E4001, result.error_msg);
        return result;
    }

    if (!result.grid) {
        result.ok = false;
        result.exit_code = ExitCode::ProcessingError;
        result.error_code = std::string(E4001);
        result.error_msg = "Grid creation returned null";
        log_error(E4001, result.error_msg);
        return result;
    }

    const int B = manifest.brick_size;
    const float bg = manifest.background_value_mm;

    // Use an accessor for efficient voxel insertion
    auto accessor = result.grid->getAccessor();

    int64_t total_set = 0;
    int64_t skipped_bg = 0;

    for (const auto& brick : bricks) {
        const int base_x = brick.bx * B;
        const int base_y = brick.by * B;
        const int base_z = brick.bz * B;

        for (int lz = 0; lz < B; ++lz) {
            for (int ly = 0; ly < B; ++ly) {
                for (int lx = 0; lx < B; ++lx) {
                    // x-fastest: index = lx + B*(ly + B*lz)
                    size_t idx = static_cast<size_t>(lx + B * (ly + B * lz));
                    float val = brick.values[idx];

                    // Skip background values (they are the grid default)
                    if (val == bg) {
                        ++skipped_bg;
                        continue;
                    }

                    openvdb::Coord ijk(base_x + lx, base_y + ly, base_z + lz);
                    accessor.setValue(ijk, val);
                    ++total_set;
                }
            }
        }
    }

    result.active_voxel_count = static_cast<int64_t>(result.grid->activeVoxelCount());

    log_info("GENMESH_I0002", "VDB grid built", {
        {"active_voxels", std::to_string(result.active_voxel_count)},
        {"set_voxels", std::to_string(total_set)},
        {"skipped_bg", std::to_string(skipped_bg)},
        {"bricks", std::to_string(bricks.size())},
    });

    result.ok = true;
    result.exit_code = ExitCode::Success;
    return result;
}

}  // namespace genmesh
