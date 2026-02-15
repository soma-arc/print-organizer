// T3.1 debug-generate sphere/box tests
#include <cassert>
#include <cmath>
#include <iostream>
#include <string>

#include "genmesh/debug_generate.h"
#include "genmesh/log.h"

void test_sphere_generates_valid_manifest() {
    auto r = genmesh::debug_generate("sphere", 64, 1.0f);
    assert(r.ok);

    auto& m = r.manifest;
    assert(m.version == 1);
    assert(m.handedness == "right");
    assert(m.up_axis == "Y");
    assert(m.front_axis == "+Z");
    assert(m.units == "mm");
    assert(m.voxel_size == 1.0f);
    assert(m.dims[0] == 64 && m.dims[1] == 64 && m.dims[2] == 64);
    assert(m.aabb_size[0] == 64.0f);
    assert(m.brick_size == 64);
    assert(m.dtype == "f32");
    assert(m.sample_at == "voxel_center");
    assert(m.axis_order == "x-fastest");
    assert(m.distance_sign == "negative_inside_positive_outside");
    assert(m.iso == 0.0f);
    assert(m.adaptivity == 0.0f);
    assert(m.half_width_voxels == 3);
    assert(m.background_value_mm == 1000.0f);

    std::cout << "  PASS: test_sphere_generates_valid_manifest\n";
}

void test_sphere_has_bricks() {
    auto r = genmesh::debug_generate("sphere", 64, 1.0f);
    assert(r.ok);
    // 64/64 = 1 brick per axis, at least 1 active
    assert(!r.bricks.empty());
    // For a 64^3 grid with B=64, there's exactly 1 brick
    assert(r.bricks.size() == 1);
    assert(r.bricks[0].bx == 0);
    assert(r.bricks[0].by == 0);
    assert(r.bricks[0].bz == 0);
    assert(r.bricks[0].values.size() == 64 * 64 * 64);

    std::cout << "  PASS: test_sphere_has_bricks\n";
}

void test_sphere_sdf_values() {
    auto r = genmesh::debug_generate("sphere", 64, 1.0f);
    assert(r.ok);

    // Sphere: center=(32,32,32), radius=64*0.4=25.6
    // Voxel at center (31,31,31) → world (31.5,31.5,31.5) → near center
    // distance ~ sqrt(0.5^2*3) - 25.6 ≈ 0.866 - 25.6 < 0 (inside)
    const auto& v = r.bricks[0].values;

    // x-fastest index: lx + 64*(ly + 64*lz)
    auto idx = [](int x, int y, int z) -> size_t {
        return static_cast<size_t>(x + 64 * (y + 64 * z));
    };

    // Center voxel should be negative (inside)
    float center_val = v[idx(31, 31, 31)];
    assert(center_val < 0.0f);

    // Corner voxel (0,0,0) → world (0.5,0.5,0.5) → far from center → positive (outside)
    float corner_val = v[idx(0, 0, 0)];
    assert(corner_val > 0.0f);

    std::cout << "  PASS: test_sphere_sdf_values\n";
}

void test_box_generates() {
    auto r = genmesh::debug_generate("box", 64, 1.0f);
    assert(r.ok);
    assert(!r.bricks.empty());

    // Box: center=(32,32,32), half-extents=64*0.3=19.2
    const auto& v = r.bricks[0].values;

    auto idx = [](int x, int y, int z) -> size_t {
        return static_cast<size_t>(x + 64 * (y + 64 * z));
    };

    // Center should be inside (negative)
    float center_val = v[idx(31, 31, 31)];
    assert(center_val < 0.0f);

    // Corner should be outside (positive)
    float corner_val = v[idx(0, 0, 0)];
    assert(corner_val > 0.0f);

    std::cout << "  PASS: test_box_generates\n";
}

void test_box_sdf_known_point() {
    // Box centered at (32,32,32), half-extent=19.2
    // A point on one face: (32+19.2, 32, 32) → distance ≈ 0
    // Voxel (51,31,31) → world (51.5,31.5,31.5)
    //   dx = |51.5-32|-19.2 = 19.5-19.2 = 0.3
    //   dy = |31.5-32|-19.2 = 0.5-19.2 = -18.7
    //   dz = same as dy = -18.7
    //   outside = sqrt(0.3^2 + 0 + 0) = 0.3
    //   inside = min(max(0.3, -18.7, -18.7), 0) = 0
    //   result = 0.3 (slightly outside)
    auto r = genmesh::debug_generate("box", 64, 1.0f);
    assert(r.ok);
    auto idx = [](int x, int y, int z) -> size_t {
        return static_cast<size_t>(x + 64 * (y + 64 * z));
    };
    float val = r.bricks[0].values[idx(51, 31, 31)];
    assert(std::abs(val - 0.3f) < 0.01f);

    std::cout << "  PASS: test_box_sdf_known_point\n";
}

void test_unknown_shape() {
    auto r = genmesh::debug_generate("cylinder", 64, 1.0f);
    assert(!r.ok);
    assert(r.exit_code == genmesh::ExitCode::General);

    std::cout << "  PASS: test_unknown_shape\n";
}

void test_multi_brick_grid() {
    // dims=128, B=64 → 2 bricks per axis = 8 total
    auto r = genmesh::debug_generate("sphere", 128, 1.0f);
    assert(r.ok);
    // All 8 bricks should contain some non-background data for a sphere
    // centered at (64,64,64) with r=128*0.4=51.2 — touches all octants
    assert(r.bricks.size() > 0);
    // Verify brick coordinates span [0,1] in each axis
    bool found00 = false, found11 = false;
    for (const auto& b : r.bricks) {
        if (b.bx == 0 && b.by == 0 && b.bz == 0) found00 = true;
        if (b.bx == 1 && b.by == 1 && b.bz == 1) found11 = true;
    }
    assert(found00);
    assert(found11);

    std::cout << "  PASS: test_multi_brick_grid\n";
}

void test_custom_voxel_size() {
    auto r = genmesh::debug_generate("sphere", 64, 0.5f);
    assert(r.ok);
    assert(r.manifest.voxel_size == 0.5f);
    assert(r.manifest.aabb_size[0] == 32.0f);  // 64 * 0.5

    std::cout << "  PASS: test_custom_voxel_size\n";
}

int main() {
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T3.1 debug-generate tests ===\n";

    test_sphere_generates_valid_manifest();
    test_sphere_has_bricks();
    test_sphere_sdf_values();
    test_box_generates();
    test_box_sdf_known_point();
    test_unknown_shape();
    test_multi_brick_grid();
    test_custom_voxel_size();

    std::cout << "=== All T3.1 tests passed ===\n";
    return 0;
}
