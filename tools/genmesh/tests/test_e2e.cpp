/// @file test_e2e.cpp
/// Phase 7: End-to-end integration tests.
///   T7.1  – fixture-based pipeline (manifest + bricks.index + bricks.bin)
///   T7.2  – debug-generate sphere → mesh.stl + report.json
///   T7.3  – regression baseline (tri/vertex/quad counts, mesh AABB)

#include "genmesh/bricks_data.h"
#include "genmesh/bricks_index.h"
#include "genmesh/debug_generate.h"
#include "genmesh/error_code.h"
#include "genmesh/exit_code.h"
#include "genmesh/log.h"
#include "genmesh/manifest.h"
#include "genmesh/mesher.h"
#include "genmesh/output.h"
#include "genmesh/report.h"
#include "genmesh/vdb_builder.h"

#include <nlohmann/json.hpp>

#include <algorithm>
#include <cassert>
#include <cmath>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace fs = std::filesystem;

static int tests_run = 0;
static int tests_passed = 0;

#define RUN(fn)                                                \
    do {                                                       \
        ++tests_run;                                           \
        std::cout << "  " << #fn << " ... ";                   \
        try {                                                  \
            fn();                                              \
            ++tests_passed;                                    \
            std::cout << "OK\n";                               \
        } catch (const std::exception& e) {                    \
            std::cout << "FAIL: " << e.what() << "\n";         \
        }                                                      \
    } while (0)

#define ASSERT(expr)                                            \
    do {                                                        \
        if (!(expr))                                            \
            throw std::runtime_error(                           \
                std::string("Assertion failed: ") + #expr +     \
                " at line " + std::to_string(__LINE__));         \
    } while (0)

// ========== helpers ==========

static fs::path fixture_dir() {
    fs::path p(__FILE__);
    return p.parent_path() / "fixtures";
}

static fs::path make_temp_dir(const std::string& tag) {
    auto p = fs::temp_directory_path() / ("genmesh_e2e_" + tag);
    fs::create_directories(p);
    return p;
}

/// Generate sphere SDF matching debug_generate(sphere, 64, 1.0) parameters.
/// N=64, voxel_size=1.0, center=32, radius=25.6, x-fastest order.
static std::vector<float> generate_sphere_sdf_64() {
    const int N = 64;
    const float vs = 1.0f;
    const float center = N * vs * 0.5f;   // 32.0
    const float radius = N * vs * 0.4f;   // 25.6
    const float bg = 1000.0f;

    std::vector<float> data(N * N * N);
    for (int z = 0; z < N; ++z) {
        for (int y = 0; y < N; ++y) {
            for (int x = 0; x < N; ++x) {
                float wx = vs * (x + 0.5f);
                float wy = vs * (y + 0.5f);
                float wz = vs * (z + 0.5f);
                float dx = wx - center;
                float dy = wy - center;
                float dz = wz - center;
                float d = std::sqrt(dx * dx + dy * dy + dz * dz) - radius;
                if (d > bg) d = bg;
                if (d < -bg) d = -bg;
                data[static_cast<size_t>(x + N * (y + N * z))] = d;
            }
        }
    }
    return data;
}

/// Write a complete valid fixture set (manifest + bricks index + bricks.bin)
/// to the given directory.  Returns the manifest path.
static std::string write_valid_fixture_set(const fs::path& dir) {
    // Copy valid_manifest.json
    auto src_manifest = fixture_dir() / "valid_manifest.json";
    auto dst_manifest = dir / "project.json";
    fs::copy_file(src_manifest, dst_manifest, fs::copy_options::overwrite_existing);

    // Copy valid_bricks_index.json
    auto src_index = fixture_dir() / "valid_bricks_index.json";
    auto dst_index = dir / "bricks.index.json";
    fs::copy_file(src_index, dst_index, fs::copy_options::overwrite_existing);

    // Generate bricks.bin (sphere SDF, 64^3 floats)
    auto sdf = generate_sphere_sdf_64();
    auto bin_path = dir / "bricks.bin";
    {
        std::ofstream ofs(bin_path, std::ios::binary);
        ofs.write(reinterpret_cast<const char*>(sdf.data()),
                  static_cast<std::streamsize>(sdf.size() * sizeof(float)));
    }
    ASSERT(fs::file_size(bin_path) == 64 * 64 * 64 * 4);

    return dst_manifest.string();
}

// ========== shared pipeline state ==========

/// Run the full debug-generate → VDB → mesh pipeline.
/// Stores results for multiple tests to share.
struct PipelineResult {
    genmesh::DebugGenerateResult dg;
    genmesh::VdbBuildResult vdb;
    genmesh::MeshResult mesh;
    genmesh::Report report;
    bool ran = false;
};

static PipelineResult& shared_sphere_result() {
    static PipelineResult pr;
    if (!pr.ran) {
        pr.ran = true;

        genmesh::ScopedTimer total_timer;

        // debug-generate
        genmesh::ScopedTimer validate_timer;
        pr.dg = genmesh::debug_generate("sphere");
        ASSERT(pr.dg.ok);

        // VDB
        genmesh::ScopedTimer vdb_timer;
        pr.vdb = genmesh::build_vdb(pr.dg.manifest, pr.dg.bricks);
        ASSERT(pr.vdb.ok);

        // Mesh
        genmesh::ScopedTimer mesh_timer;
        pr.mesh = genmesh::extract_mesh(pr.vdb.grid, 0.0, 0.0);
        ASSERT(pr.mesh.ok);

        // Build report
        pr.report.schema_version = 1;
        pr.report.status = "success";
        pr.report.stage = genmesh::Stage::Write;
        pr.report.started_at_utc = genmesh::utc_now_iso8601();

        pr.report.inputs.manifest_path = "(debug-generate)";
        pr.report.inputs.in_dir = "";
        pr.report.inputs.dtype = pr.dg.manifest.dtype;
        pr.report.inputs.brick_size = pr.dg.manifest.brick_size;
        pr.report.inputs.dims = pr.dg.manifest.dims;
        pr.report.inputs.voxel_size = pr.dg.manifest.voxel_size;

        pr.report.timing_ms.total = total_timer.elapsed_ms();
        pr.report.timing_ms.validate = validate_timer.elapsed_ms();
        pr.report.timing_ms.read = 0.0;
        pr.report.timing_ms.vdb_build = vdb_timer.elapsed_ms();
        pr.report.timing_ms.meshing = mesh_timer.elapsed_ms();

        const auto& m = pr.dg.manifest;
        pr.report.stats.aabb_min = m.aabb_min;
        pr.report.stats.aabb_max = {
            m.aabb_min[0] + m.aabb_size[0],
            m.aabb_min[1] + m.aabb_size[1],
            m.aabb_min[2] + m.aabb_size[2],
        };
        pr.report.stats.brick_count = static_cast<int64_t>(pr.dg.bricks.size());
        pr.report.stats.triangle_count = static_cast<int64_t>(pr.mesh.mesh.triangles.size());
        pr.report.stats.quad_count = pr.mesh.mesh.original_quad_count;
        pr.report.stats.vertex_count = static_cast<int64_t>(pr.mesh.mesh.points.size());
        pr.report.stats.degenerate_count = pr.mesh.mesh.degenerate_count;
        pr.report.stats.active_voxel_count = pr.vdb.active_voxel_count;

        // Compute mesh AABB
        if (!pr.mesh.mesh.points.empty()) {
            float mn_x = pr.mesh.mesh.points[0][0];
            float mn_y = pr.mesh.mesh.points[0][1];
            float mn_z = pr.mesh.mesh.points[0][2];
            float mx_x = mn_x, mx_y = mn_y, mx_z = mn_z;
            for (const auto& p : pr.mesh.mesh.points) {
                if (p[0] < mn_x) mn_x = p[0]; if (p[0] > mx_x) mx_x = p[0];
                if (p[1] < mn_y) mn_y = p[1]; if (p[1] > mx_y) mx_y = p[1];
                if (p[2] < mn_z) mn_z = p[2]; if (p[2] > mx_z) mx_z = p[2];
            }
            pr.report.stats.has_mesh_aabb = true;
            pr.report.stats.mesh_aabb_min = {mn_x, mn_y, mn_z};
            pr.report.stats.mesh_aabb_max = {mx_x, mx_y, mx_z};
        }

        pr.report.ended_at_utc = genmesh::utc_now_iso8601();
    }
    return pr;
}

// ===================================================================
//  T7.2  E2E tests — debug-generate sphere pipeline
// ===================================================================

void test_e2e_debug_generate_sphere_ok() {
    auto& pr = shared_sphere_result();
    ASSERT(pr.dg.ok);
    ASSERT(pr.vdb.ok);
    ASSERT(pr.mesh.ok);
    ASSERT(!pr.mesh.mesh.triangles.empty());
    ASSERT(!pr.mesh.mesh.points.empty());
}

void test_e2e_stl_output() {
    auto& pr = shared_sphere_result();
    auto dir = make_temp_dir("stl_out");
    auto stl_path = dir / "mesh.stl";

    auto res = genmesh::write_stl(stl_path, pr.mesh.mesh);
    ASSERT(res.ok);
    ASSERT(fs::exists(stl_path));
    // STL header(80) + tri_count(4) + tri*(50) = 80 + 4 + 24672*50 = 1233684
    ASSERT(fs::file_size(stl_path) > 1000);

    fs::remove_all(dir);
}

void test_e2e_vdb_output() {
    auto& pr = shared_sphere_result();
    auto dir = make_temp_dir("vdb_out");
    auto vdb_path = dir / "volume.vdb";

    auto res = genmesh::write_vdb(vdb_path, pr.vdb.grid);
    ASSERT(res.ok);
    ASSERT(fs::exists(vdb_path));
    ASSERT(fs::file_size(vdb_path) > 100);

    fs::remove_all(dir);
}

void test_e2e_report_output() {
    auto& pr = shared_sphere_result();
    auto dir = make_temp_dir("report_out");
    auto rpt_path = dir / "report.json";

    auto wr = genmesh::write_report(rpt_path, pr.report);
    ASSERT(wr.ok);
    ASSERT(fs::exists(rpt_path));
    ASSERT(fs::file_size(rpt_path) > 50);

    // Parse and validate
    std::ifstream ifs(rpt_path);
    auto j = nlohmann::json::parse(ifs);
    ifs.close();

    ASSERT(j["schema_version"] == 1);
    ASSERT(j["status"] == "success");
    ASSERT(j["stage"] == "write");
    ASSERT(j.contains("timing_ms"));
    ASSERT(j.contains("stats"));
    ASSERT(j.contains("inputs"));
    ASSERT(j.contains("warnings"));
    ASSERT(j.contains("errors"));
    ASSERT(j["errors"].empty());

    fs::remove_all(dir);
}

void test_e2e_report_stats_positive() {
    auto& pr = shared_sphere_result();
    auto j = genmesh::report_to_json(pr.report);
    auto s = j["stats"];

    ASSERT(s["triangle_count"].get<int64_t>() > 0);
    ASSERT(s["vertex_count"].get<int64_t>() > 0);
    ASSERT(s["quad_count"].get<int64_t>() > 0);
    ASSERT(s["brick_count"].get<int64_t>() > 0);
    ASSERT(s["degenerate_count"].get<int64_t>() >= 0);
    ASSERT(s["active_voxel_count"].get<int64_t>() > 0);
    ASSERT(s.contains("mesh_aabb_min"));
    ASSERT(s.contains("mesh_aabb_max"));
}

void test_e2e_report_timing_all_stages() {
    auto& pr = shared_sphere_result();
    auto j = genmesh::report_to_json(pr.report);
    auto t = j["timing_ms"];

    ASSERT(t.contains("total"));
    ASSERT(t.contains("validate"));
    ASSERT(t.contains("read"));
    ASSERT(t.contains("vdb_build"));
    ASSERT(t.contains("meshing"));
    ASSERT(t["total"].get<double>() > 0.0);
}

// ===================================================================
//  T7.3  Regression baselines — sphere fixture
// ===================================================================

// Known values from debug_generate("sphere", 64, 1.0) with OpenVDB mesher:
//   triangle_count = 24672
//   vertex_count   = 12338
//   quad_count      = 12336
//   degenerate_count = 0
//   active_voxel_count = 262144
//   mesh_aabb_min  ≈ (5.91, 5.91, 5.91)
//   mesh_aabb_max  ≈ (57.09, 57.09, 57.09)

void test_baseline_sphere_triangle_count() {
    auto& pr = shared_sphere_result();
    int64_t tri = static_cast<int64_t>(pr.mesh.mesh.triangles.size());
    ASSERT(tri == 24672);
}

void test_baseline_sphere_vertex_count() {
    auto& pr = shared_sphere_result();
    int64_t vtx = static_cast<int64_t>(pr.mesh.mesh.points.size());
    ASSERT(vtx == 12338);
}

void test_baseline_sphere_quad_count() {
    auto& pr = shared_sphere_result();
    ASSERT(pr.mesh.mesh.original_quad_count == 12336);
}

void test_baseline_sphere_degenerate_zero() {
    auto& pr = shared_sphere_result();
    ASSERT(pr.mesh.mesh.degenerate_count == 0);
}

void test_baseline_sphere_active_voxels() {
    auto& pr = shared_sphere_result();
    ASSERT(pr.vdb.active_voxel_count == 262144);
}

void test_baseline_sphere_mesh_aabb_symmetry() {
    auto& pr = shared_sphere_result();
    ASSERT(pr.report.stats.has_mesh_aabb);

    const auto& mn = pr.report.stats.mesh_aabb_min;
    const auto& mx = pr.report.stats.mesh_aabb_max;

    // Sphere is centered at (32, 32, 32) so AABB should be symmetric.
    // min should be roughly 32 - 25.6 = 6.4 (discretization shifts slightly).
    // Allow tolerance of 2.0 voxels.
    for (int i = 0; i < 3; ++i) {
        ASSERT(mn[i] > 3.0f && mn[i] < 10.0f);
        ASSERT(mx[i] > 54.0f && mx[i] < 61.0f);
        // Symmetry: min[i] + max[i] ≈ 64
        float sum = mn[i] + mx[i];
        ASSERT(std::abs(sum - 64.0f) < 2.0f);
    }
}

void test_baseline_sphere_mesh_aabb_exact() {
    // Exact known values from E2E run (rounded to 2 decimal places for tolerance)
    auto& pr = shared_sphere_result();
    const auto& mn = pr.report.stats.mesh_aabb_min;
    const auto& mx = pr.report.stats.mesh_aabb_max;

    // XYZ should all be identical for a sphere on a cubic grid.
    ASSERT(std::abs(mn[0] - mn[1]) < 0.001f);
    ASSERT(std::abs(mn[1] - mn[2]) < 0.001f);
    ASSERT(std::abs(mx[0] - mx[1]) < 0.001f);
    ASSERT(std::abs(mx[1] - mx[2]) < 0.001f);

    // Known: min ≈ 5.9098, max ≈ 57.0902  (tolerance 0.01)
    ASSERT(std::abs(mn[0] - 5.9098f) < 0.01f);
    ASSERT(std::abs(mx[0] - 57.0902f) < 0.01f);
}

// ===================================================================
//  T7.1  Fixture-based pipeline (file path)
// ===================================================================

void test_e2e_file_pipeline() {
    auto dir = make_temp_dir("file_pipeline");

    // Write valid fixture set
    auto manifest_path = write_valid_fixture_set(dir);

    // Load manifest
    auto mr = genmesh::load_manifest(manifest_path);
    ASSERT(mr.ok);

    // Load bricks index
    auto idx_path = (dir / "bricks.index.json").string();
    auto ir = genmesh::load_bricks_index(idx_path, mr.manifest);
    ASSERT(ir.ok);

    // Load bricks data
    auto bin_path = (dir / "bricks.bin").string();
    auto br = genmesh::load_bricks_bin(bin_path, ir.index, mr.manifest);
    ASSERT(br.ok);
    ASSERT(!br.bricks.empty());

    // VDB build
    auto vdb = genmesh::build_vdb(mr.manifest, br.bricks);
    ASSERT(vdb.ok);
    ASSERT(vdb.active_voxel_count > 0);

    // Mesh
    auto mesh = genmesh::extract_mesh(vdb.grid, 0.0, 0.0);
    ASSERT(mesh.ok);
    ASSERT(!mesh.mesh.triangles.empty());
    ASSERT(!mesh.mesh.points.empty());

    // Write STL
    auto out_dir = dir / "out";
    fs::create_directories(out_dir);
    auto stl_res = genmesh::write_stl(out_dir / "mesh.stl", mesh.mesh);
    ASSERT(stl_res.ok);
    ASSERT(fs::exists(out_dir / "mesh.stl"));
    ASSERT(fs::file_size(out_dir / "mesh.stl") > 0);

    // Tri count should match the debug-generate sphere baseline
    // (since the SDF is generated with the same formula)
    ASSERT(static_cast<int64_t>(mesh.mesh.triangles.size()) == 24672);

    fs::remove_all(dir);
}

void test_e2e_invalid_manifest_no_dims() {
    auto manifest_path = (fixture_dir() / "invalid_manifest_no_dims.json").string();
    auto mr = genmesh::load_manifest(manifest_path);
    ASSERT(!mr.ok);
    ASSERT(mr.exit_code == genmesh::ExitCode::ValidationFailure);
    ASSERT(!mr.errors.empty());

    // Should have E1001 (required field missing)
    bool found_e1001 = false;
    for (const auto& e : mr.errors) {
        if (e.code == "GENMESH_E1001") found_e1001 = true;
    }
    ASSERT(found_e1001);
}

void test_e2e_invalid_manifest_bad_dtype() {
    auto manifest_path = (fixture_dir() / "invalid_manifest_bad_dtype.json").string();
    auto mr = genmesh::load_manifest(manifest_path);
    ASSERT(!mr.ok);
    ASSERT(!mr.errors.empty());
}

void test_e2e_failure_report_is_written() {
    auto dir = make_temp_dir("fail_report");
    fs::create_directories(dir);

    // Build a failure report
    genmesh::Report report;
    report.schema_version = 1;
    report.status = "failure";
    report.stage = genmesh::Stage::Validate;
    report.started_at_utc = genmesh::utc_now_iso8601();
    report.ended_at_utc = genmesh::utc_now_iso8601();
    report.inputs.manifest_path = "bad.json";
    report.inputs.in_dir = dir.string();
    report.inputs.dtype = "f32";
    report.inputs.brick_size = 64;
    report.inputs.dims = {64, 64, 64};
    report.inputs.voxel_size = 1.0f;
    report.has_progress = true;
    report.progress.stage = genmesh::Stage::Validate;
    report.errors.push_back({
        "GENMESH_E1001", "Missing field: dims", "validation", "", {}, ""
    });

    auto rpt_path = dir / "report.json";
    auto wr = genmesh::write_report(rpt_path, report);
    ASSERT(wr.ok);
    ASSERT(fs::exists(rpt_path));

    // Verify contents
    std::ifstream ifs(rpt_path);
    auto j = nlohmann::json::parse(ifs);
    ifs.close();

    ASSERT(j["status"] == "failure");
    ASSERT(j["stage"] == "validate");
    ASSERT(j["errors"].size() == 1);
    ASSERT(j["errors"][0]["code"] == "GENMESH_E1001");
    ASSERT(j.contains("progress"));
    ASSERT(j["progress"]["stage"] == "validate");

    fs::remove_all(dir);
}

// ===================================================================
//  main
// ===================================================================

int main() {
    // Suppress log noise during tests
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    // VDB init (required once before any VDB operations)
    ASSERT(genmesh::vdb_init());

    std::cout << "=== test_e2e ===\n";

    // T7.2: E2E debug-generate pipeline
    std::cout << "\n--- T7.2: E2E debug-generate ---\n";
    RUN(test_e2e_debug_generate_sphere_ok);
    RUN(test_e2e_stl_output);
    RUN(test_e2e_vdb_output);
    RUN(test_e2e_report_output);
    RUN(test_e2e_report_stats_positive);
    RUN(test_e2e_report_timing_all_stages);

    // T7.3: Regression baselines
    std::cout << "\n--- T7.3: regression baselines ---\n";
    RUN(test_baseline_sphere_triangle_count);
    RUN(test_baseline_sphere_vertex_count);
    RUN(test_baseline_sphere_quad_count);
    RUN(test_baseline_sphere_degenerate_zero);
    RUN(test_baseline_sphere_active_voxels);
    RUN(test_baseline_sphere_mesh_aabb_symmetry);
    RUN(test_baseline_sphere_mesh_aabb_exact);

    // T7.1: Fixture-based pipeline
    std::cout << "\n--- T7.1: fixture-based pipeline ---\n";
    RUN(test_e2e_file_pipeline);
    RUN(test_e2e_invalid_manifest_no_dims);
    RUN(test_e2e_invalid_manifest_bad_dtype);
    RUN(test_e2e_failure_report_is_written);

    std::cout << "\n" << tests_passed << "/" << tests_run << " passed\n";
    return (tests_passed == tests_run) ? 0 : 1;
}
