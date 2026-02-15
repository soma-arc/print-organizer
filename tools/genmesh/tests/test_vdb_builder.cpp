// T4.1 + T4.2 VDB builder tests
#include <cassert>
#include <cmath>
#include <iostream>
#include <string>

#include <openvdb/openvdb.h>

#include "genmesh/debug_generate.h"
#include "genmesh/log.h"
#include "genmesh/manifest.h"
#include "genmesh/vdb_builder.h"

void test_vdb_init() {
    bool ok = genmesh::vdb_init();
    assert(ok);
    std::cout << "  PASS: test_vdb_init\n";
}

void test_create_grid_transform() {
    genmesh::Manifest m;
    m.voxel_size = 2.0f;
    m.aabb_min = {10.0f, 20.0f, 30.0f};
    m.background_value_mm = 500.0f;
    m.brick_size = 64;
    m.dims = {64, 64, 64};

    auto grid = genmesh::create_grid(m);
    assert(grid != nullptr);

    // Check background value
    assert(grid->background() == 500.0f);

    // Check grid class
    assert(grid->getGridClass() == openvdb::GRID_LEVEL_SET);

    // Check transform: index (0,0,0) → world should be aabb_min + 0.5*voxel_size
    // Because OpenVDB's indexToWorld for center-of-voxel:
    //   world = voxelSize * index + translation
    // With our postTranslate by aabb_min:
    //   world(0,0,0) = voxelSize * (0,0,0) + aabb_min = aabb_min
    // But we want voxel CENTER at aabb_min + 0.5*vs.
    // OpenVDB's linear transform maps index → world as: world = vs * index
    // Then postTranslate adds aabb_min. So index (0,0,0) → (0,0,0) + aabb_min = aabb_min.
    // The voxel center convention (+0.5) is handled by the sampling, not the transform.
    auto worldPos = grid->indexToWorld(openvdb::Coord(0, 0, 0));
    assert(std::abs(worldPos.x() - 10.0) < 1e-6);
    assert(std::abs(worldPos.y() - 20.0) < 1e-6);
    assert(std::abs(worldPos.z() - 30.0) < 1e-6);

    // Check voxel size
    auto vs = grid->voxelSize();
    assert(std::abs(vs.x() - 2.0) < 1e-6);

    // Index (1,0,0) → world should be (10+2, 20, 30) = (12, 20, 30)
    auto worldPos1 = grid->indexToWorld(openvdb::Coord(1, 0, 0));
    assert(std::abs(worldPos1.x() - 12.0) < 1e-6);

    std::cout << "  PASS: test_create_grid_transform\n";
}

void test_create_grid_default_aabb_min() {
    genmesh::Manifest m;
    m.voxel_size = 1.0f;
    m.aabb_min = {0.0f, 0.0f, 0.0f};
    m.background_value_mm = 1000.0f;
    m.brick_size = 64;
    m.dims = {64, 64, 64};

    auto grid = genmesh::create_grid(m);
    auto worldPos = grid->indexToWorld(openvdb::Coord(0, 0, 0));
    assert(std::abs(worldPos.x()) < 1e-6);
    assert(std::abs(worldPos.y()) < 1e-6);
    assert(std::abs(worldPos.z()) < 1e-6);

    std::cout << "  PASS: test_create_grid_default_aabb_min\n";
}

void test_build_vdb_sphere() {
    // Generate sphere SDF
    auto gen = genmesh::debug_generate("sphere", 64, 1.0f);
    assert(gen.ok);

    // Build VDB
    auto r = genmesh::build_vdb(gen.manifest, gen.bricks);
    assert(r.ok);
    assert(r.grid != nullptr);
    assert(r.active_voxel_count > 0);

    // Sphere has center at (32,32,32), radius=25.6
    // Check a known inside point: index (31,31,31) → should be negative
    auto accessor = r.grid->getConstAccessor();
    float center_val = accessor.getValue(openvdb::Coord(31, 31, 31));
    assert(center_val < 0.0f);

    // Check a corner: index (0,0,0) → should be positive (outside)
    float corner_val = accessor.getValue(openvdb::Coord(0, 0, 0));
    assert(corner_val > 0.0f);

    std::cout << "  PASS: test_build_vdb_sphere\n";
}

void test_build_vdb_box() {
    auto gen = genmesh::debug_generate("box", 64, 1.0f);
    assert(gen.ok);

    auto r = genmesh::build_vdb(gen.manifest, gen.bricks);
    assert(r.ok);
    assert(r.active_voxel_count > 0);

    auto accessor = r.grid->getConstAccessor();
    // Box center at (32,32,32), half-extent=19.2
    // Center should be negative
    float center_val = accessor.getValue(openvdb::Coord(31, 31, 31));
    assert(center_val < 0.0f);

    std::cout << "  PASS: test_build_vdb_box\n";
}

void test_build_vdb_multi_brick() {
    // 128^3 grid → 2x2x2 = 8 bricks
    auto gen = genmesh::debug_generate("sphere", 128, 1.0f);
    assert(gen.ok);
    assert(gen.bricks.size() > 1);

    auto r = genmesh::build_vdb(gen.manifest, gen.bricks);
    assert(r.ok);
    assert(r.active_voxel_count > 0);

    // Sphere center at (64,64,64), radius=51.2
    auto accessor = r.grid->getConstAccessor();
    float center_val = accessor.getValue(openvdb::Coord(63, 63, 63));
    assert(center_val < 0.0f);

    std::cout << "  PASS: test_build_vdb_multi_brick\n";
}

void test_build_vdb_empty_bricks() {
    genmesh::Manifest m;
    m.version = 1;
    m.voxel_size = 1.0f;
    m.aabb_min = {0, 0, 0};
    m.aabb_size = {64, 64, 64};
    m.dims = {64, 64, 64};
    m.brick_size = 64;
    m.dtype = "f32";
    m.background_value_mm = 1000.0f;

    // No bricks → all background
    std::vector<genmesh::BrickData> empty;
    auto r = genmesh::build_vdb(m, empty);
    assert(r.ok);
    assert(r.active_voxel_count == 0);

    // Any voxel should return background
    auto accessor = r.grid->getConstAccessor();
    float val = accessor.getValue(openvdb::Coord(0, 0, 0));
    assert(val == 1000.0f);

    std::cout << "  PASS: test_build_vdb_empty_bricks\n";
}

int main() {
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T4 VDB builder tests ===\n";

    test_vdb_init();
    test_create_grid_transform();
    test_create_grid_default_aabb_min();
    test_build_vdb_sphere();
    test_build_vdb_box();
    test_build_vdb_multi_brick();
    test_build_vdb_empty_bricks();

    std::cout << "=== All T4 tests passed ===\n";
    return 0;
}
