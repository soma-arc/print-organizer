#include "genmesh/debug_generate.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

namespace genmesh {

// ---------- SDF primitives ----------

/// Signed distance to a sphere centered at `center` with radius `r`.
static float sdf_sphere(float x, float y, float z,
                        float cx, float cy, float cz, float r) {
    float dx = x - cx;
    float dy = y - cy;
    float dz = z - cz;
    return std::sqrt(dx * dx + dy * dy + dz * dz) - r;
}

/// Signed distance to an axis-aligned box centered at `center` with half-extents `hx,hy,hz`.
static float sdf_box(float x, float y, float z,
                     float cx, float cy, float cz,
                     float hx, float hy, float hz) {
    float dx = std::abs(x - cx) - hx;
    float dy = std::abs(y - cy) - hy;
    float dz = std::abs(z - cz) - hz;
    float outside = std::sqrt(
        std::max(dx, 0.0f) * std::max(dx, 0.0f) +
        std::max(dy, 0.0f) * std::max(dy, 0.0f) +
        std::max(dz, 0.0f) * std::max(dz, 0.0f));
    float inside = std::min(std::max({dx, dy, dz}), 0.0f);
    return outside + inside;
}

// ---------- generate ----------

DebugGenerateResult debug_generate(const std::string& shape,
                                   int dims,
                                   float voxel_size) {
    DebugGenerateResult result;

    if (shape != "sphere" && shape != "box") {
        result.ok = false;
        result.exit_code = ExitCode::General;
        result.error_msg = "Unknown debug shape: " + shape;
        log_error(E9001, result.error_msg);
        return result;
    }

    const int B = 64;  // brick size
    const int N = dims; // grid dims per axis (assume cubic)
    const float vs = voxel_size;

    // --- Build manifest ---
    auto& m = result.manifest;
    m.version = 1;
    m.handedness = "right";
    m.up_axis = "Y";
    m.front_axis = "+Z";
    m.units = "mm";
    m.aabb_min = {0.0f, 0.0f, 0.0f};
    m.aabb_size = {N * vs, N * vs, N * vs};
    m.voxel_size = vs;
    m.dims = {N, N, N};
    m.sample_at = "voxel_center";
    m.axis_order = "x-fastest";
    m.distance_sign = "negative_inside_positive_outside";
    m.iso = 0.0f;
    m.adaptivity = 0.0f;
    m.half_width_voxels = 3;
    m.brick_size = B;
    m.dtype = "f32";
    m.background_value_mm = 1000.0f;

    // --- SDF parameters ---
    const float extent = N * vs;
    const float center = extent * 0.5f;  // center of the grid

    // Sphere: radius = 40% of extent
    const float sphere_r = extent * 0.4f;

    // Box: half-extent = 30% of extent per axis
    const float box_h = extent * 0.3f;

    // --- Generate bricks ---
    const int bricks_per_axis = static_cast<int>(std::ceil(static_cast<double>(N) / B));

    log_info("GENMESH_I0001", "debug-generate: shape=" + shape, {
        {"dims", std::to_string(N)},
        {"voxel_size", std::to_string(vs)},
        {"bricks_per_axis", std::to_string(bricks_per_axis)},
    });

    for (int bz = 0; bz < bricks_per_axis; ++bz) {
        for (int by = 0; by < bricks_per_axis; ++by) {
            for (int bx = 0; bx < bricks_per_axis; ++bx) {
                BrickData bd;
                bd.bx = bx;
                bd.by = by;
                bd.bz = bz;

                // Actual voxels in this brick (handle boundary bricks)
                const int local_B = B;  // always dense B^3

                bd.values.resize(static_cast<size_t>(local_B) * local_B * local_B);

                bool all_background = true;
                const float band = m.half_width_voxels * vs;

                for (int lz = 0; lz < local_B; ++lz) {
                    for (int ly = 0; ly < local_B; ++ly) {
                        for (int lx = 0; lx < local_B; ++lx) {
                            // Global voxel index
                            int gx = bx * B + lx;
                            int gy = by * B + ly;
                            int gz = bz * B + lz;

                            // World position (voxel center)
                            float wx = m.aabb_min[0] + vs * (gx + 0.5f);
                            float wy = m.aabb_min[1] + vs * (gy + 0.5f);
                            float wz = m.aabb_min[2] + vs * (gz + 0.5f);

                            float d;
                            if (shape == "sphere") {
                                d = sdf_sphere(wx, wy, wz, center, center, center, sphere_r);
                            } else {
                                d = sdf_box(wx, wy, wz, center, center, center,
                                            box_h, box_h, box_h);
                            }

                            // Clamp to background if outside narrow band
                            if (d > m.background_value_mm) {
                                d = m.background_value_mm;
                            } else if (d < -m.background_value_mm) {
                                d = -m.background_value_mm;
                            }

                            if (std::abs(d - m.background_value_mm) > 1e-6f) {
                                all_background = false;
                            }

                            // x-fastest: index = lx + B*(ly + B*lz)
                            size_t idx = static_cast<size_t>(lx + local_B * (ly + local_B * lz));
                            bd.values[idx] = d;
                        }
                    }
                }

                // Sparse optimization: skip all-background bricks
                if (!all_background) {
                    result.bricks.push_back(std::move(bd));
                }
            }
        }
    }

    log_info("GENMESH_I0001", "debug-generate complete", {
        {"total_bricks", std::to_string(bricks_per_axis * bricks_per_axis * bricks_per_axis)},
        {"active_bricks", std::to_string(result.bricks.size())},
    });

    result.ok = true;
    result.exit_code = ExitCode::Success;
    return result;
}

}  // namespace genmesh
