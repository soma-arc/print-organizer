#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>

#include "genmesh/bricks_data.h"
#include "genmesh/bricks_index.h"
#include "genmesh/cli.h"
#include "genmesh/debug_generate.h"
#include "genmesh/error_code.h"
#include "genmesh/exit_code.h"
#include "genmesh/log.h"
#include "genmesh/manifest.h"
#include "genmesh/mesher.h"
#include "genmesh/output.h"
#include "genmesh/report.h"
#include "genmesh/vdb_builder.h"

namespace fs = std::filesystem;

/// Helper: add an error diagnostic to the report and set failure state.
static void fail_report(genmesh::Report& report, genmesh::Stage stage,
                        const std::string& code, const std::string& kind,
                        const std::string& message) {
    report.status = "failure";
    report.stage = stage;
    report.has_progress = true;
    report.progress.stage = stage;
    report.errors.push_back({code, message, kind, "", {}, ""});
}

/// Try to write report.json; log on failure but do not change exit code.
static void try_write_report(genmesh::Report& report, const fs::path& out_dir,
                             const genmesh::ScopedTimer& total_timer) {
    report.ended_at_utc = genmesh::utc_now_iso8601();
    report.timing_ms.total = total_timer.elapsed_ms();
    genmesh::write_report(out_dir / "report.json", report);
}

int main(int argc, char* argv[]) {
    using namespace genmesh;

    ScopedTimer total_timer;

    // ---- 1. CLI parse ----
    auto parsed = parse_args(argc, argv);

    if (parsed.args.help) {
        print_usage();
        return static_cast<int>(ExitCode::Success);
    }

    if (!parsed.ok) {
        log_error(E9001, parsed.error_msg);
        return parsed.exit_code;
    }

    const auto& args = parsed.args;

    // Set global log level
    min_log_level() = parse_log_level(args.log_level);

    log_info("GENMESH_I0000", "genmesh v0.1.0 starting", {
        {"manifest", args.manifest_path},
        {"in", args.in_dir},
        {"out", args.out_dir},
    });

    // Initialize report
    Report report;
    report.started_at_utc = utc_now_iso8601();
    report.status = "success";
    report.stage = Stage::Write;  // updated on failure
    report.inputs.manifest_path = args.manifest_path;
    report.inputs.in_dir = args.in_dir;

    // ---- 2. Prepare output directory ----
    auto out_res = prepare_output_dir(args.out_dir, args.write_stl, args.write_vdb, args.force);
    if (!out_res.ok) {
        // Cannot write report if output dir is not available
        return static_cast<int>(out_res.exit_code);
    }

    fs::path out_dir(args.out_dir);

    // ---- 3. Acquire manifest + brick data ----
    Manifest manifest;
    std::vector<BrickData> bricks;

    {
        ScopedTimer validate_timer;

        if (!args.debug_generate.empty()) {
            log_info("GENMESH_I0000", "Using debug-generate mode", {
                {"shape", args.debug_generate},
            });

            auto dg = debug_generate(args.debug_generate);
            if (!dg.ok) {
                log_error(E9001, dg.error_msg);
                fail_report(report, Stage::Validate, std::string(E9001),
                            "unexpected", dg.error_msg);
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(dg.exit_code);
            }
            manifest = std::move(dg.manifest);
            bricks = std::move(dg.bricks);

            report.timing_ms.validate = validate_timer.elapsed_ms();
            // debug-generate has no read phase
            report.timing_ms.read = 0.0;
        } else {
            // 3a. Load manifest
            auto mr = load_manifest(args.manifest_path);
            if (!mr.ok) {
                for (const auto& e : mr.errors) {
                    log_error(e.code, e.message, {{"field", e.field}});
                    report.errors.push_back({e.code, e.message, "validation",
                                             "", {{"field", e.field}}, ""});
                }
                report.status = "failure";
                report.stage = Stage::Validate;
                report.has_progress = true;
                report.progress.stage = Stage::Validate;
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(mr.exit_code);
            }
            manifest = std::move(mr.manifest);

            // Apply CLI overrides
            if (args.iso.has_value()) manifest.iso = args.iso.value();
            if (args.adaptivity.has_value()) manifest.adaptivity = args.adaptivity.value();

            report.timing_ms.validate = validate_timer.elapsed_ms();

            // 3b. Load bricks index + binary
            ScopedTimer read_timer;

            auto idx_path = (fs::path(args.in_dir) / "bricks.index.json").string();
            auto ir = load_bricks_index(idx_path, manifest);
            if (!ir.ok) {
                for (const auto& e : ir.errors) {
                    log_error(e.code, e.message, {{"field", e.field}});
                    report.errors.push_back({e.code, e.message, "validation",
                                             "", {{"field", e.field}}, ""});
                }
                report.status = "failure";
                report.stage = Stage::Read;
                report.has_progress = true;
                report.progress.stage = Stage::Read;
                report.timing_ms.read = read_timer.elapsed_ms();
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(ir.exit_code);
            }

            auto bin_path = (fs::path(args.in_dir) / "bricks.bin").string();
            auto br = load_bricks_bin(bin_path, ir.index, manifest);
            if (!br.ok) {
                for (const auto& e : br.errors) {
                    log_error(e.code, e.message, {{"field", e.field}});
                    report.errors.push_back({e.code, e.message, "io",
                                             "", {{"field", e.field}}, ""});
                }
                report.status = "failure";
                report.stage = Stage::Read;
                report.has_progress = true;
                report.progress.stage = Stage::Read;
                report.timing_ms.read = read_timer.elapsed_ms();
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(br.exit_code);
            }
            bricks = std::move(br.bricks);

            report.timing_ms.read = read_timer.elapsed_ms();
        }
    }

    // Populate report inputs from manifest
    report.inputs.dtype = manifest.dtype;
    report.inputs.brick_size = manifest.brick_size;
    report.inputs.dims = manifest.dims;
    report.inputs.voxel_size = manifest.voxel_size;

    // Populate stats from manifest
    report.stats.aabb_min = manifest.aabb_min;
    report.stats.aabb_max = {
        manifest.aabb_min[0] + manifest.aabb_size[0],
        manifest.aabb_min[1] + manifest.aabb_size[1],
        manifest.aabb_min[2] + manifest.aabb_size[2],
    };
    report.stats.brick_count = static_cast<int64_t>(bricks.size());

    // ---- 4. OpenVDB init + build grid ----
    {
        ScopedTimer vdb_timer;

        if (!vdb_init()) {
            fail_report(report, Stage::VdbBuild, std::string(E3001),
                        "env", "openvdb::initialize() failed");
            try_write_report(report, out_dir, total_timer);
            return static_cast<int>(ExitCode::EnvironmentError);
        }

        auto vdb_res = build_vdb(manifest, bricks);
        if (!vdb_res.ok) {
            fail_report(report, Stage::VdbBuild, vdb_res.error_code,
                        "vdb", vdb_res.error_msg);
            report.timing_ms.vdb_build = vdb_timer.elapsed_ms();
            try_write_report(report, out_dir, total_timer);
            return static_cast<int>(vdb_res.exit_code);
        }

        report.timing_ms.vdb_build = vdb_timer.elapsed_ms();
        report.stats.active_voxel_count = vdb_res.active_voxel_count;

        // ---- 5. Mesh extraction ----
        ScopedTimer mesh_timer;

        double iso = static_cast<double>(manifest.iso);
        double adaptivity = static_cast<double>(manifest.adaptivity);

        auto mesh_res = extract_mesh(vdb_res.grid, iso, adaptivity);
        if (!mesh_res.ok) {
            fail_report(report, Stage::Meshing, mesh_res.error_code,
                        "meshing", mesh_res.error_msg);
            report.timing_ms.meshing = mesh_timer.elapsed_ms();
            try_write_report(report, out_dir, total_timer);
            return static_cast<int>(mesh_res.exit_code);
        }

        report.timing_ms.meshing = mesh_timer.elapsed_ms();

        // Populate mesh stats
        const auto& mesh = mesh_res.mesh;
        report.stats.triangle_count = static_cast<int64_t>(mesh.triangles.size());
        report.stats.quad_count = mesh.original_quad_count;
        report.stats.vertex_count = static_cast<int64_t>(mesh.points.size());
        report.stats.degenerate_count = mesh.degenerate_count;

        if (mesh.degenerate_count > 0) {
            report.warnings.push_back({
                std::string(W5001), "Degenerate triangles detected", "meshing",
                "", {{"count", mesh.degenerate_count}}, ""
            });
        }

        if (mesh.triangles.empty()) {
            log_warn(W5002, "Mesh has zero triangles");
            report.warnings.push_back({
                std::string(W5002), "Mesh has zero triangles", "meshing",
                "", {}, ""
            });
        }

        // Compute mesh AABB
        if (!mesh.points.empty()) {
            float min_x = mesh.points[0][0], min_y = mesh.points[0][1], min_z = mesh.points[0][2];
            float max_x = min_x, max_y = min_y, max_z = min_z;
            for (const auto& p : mesh.points) {
                if (p[0] < min_x) min_x = p[0]; if (p[0] > max_x) max_x = p[0];
                if (p[1] < min_y) min_y = p[1]; if (p[1] > max_y) max_y = p[1];
                if (p[2] < min_z) min_z = p[2]; if (p[2] > max_z) max_z = p[2];
            }
            report.stats.has_mesh_aabb = true;
            report.stats.mesh_aabb_min = {min_x, min_y, min_z};
            report.stats.mesh_aabb_max = {max_x, max_y, max_z};
        }

        // ---- 6. Write outputs ----
        ScopedTimer write_timer;

        // 6a. STL
        if (args.write_stl) {
            auto stl_res = write_stl(out_dir / "mesh.stl", mesh);
            if (!stl_res.ok) {
                fail_report(report, Stage::Write, stl_res.error_code,
                            "io", stl_res.error_msg);
                report.timing_ms.write = write_timer.elapsed_ms();
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(stl_res.exit_code);
            }
        }

        // 6b. VDB
        if (args.write_vdb) {
            auto vdb_wr = write_vdb(out_dir / "volume.vdb", vdb_res.grid);
            if (!vdb_wr.ok) {
                fail_report(report, Stage::Write, vdb_wr.error_code,
                            "io", vdb_wr.error_msg);
                report.timing_ms.write = write_timer.elapsed_ms();
                try_write_report(report, out_dir, total_timer);
                return static_cast<int>(vdb_wr.exit_code);
            }
        }

        report.timing_ms.write = write_timer.elapsed_ms();
    }

    // ---- 7. Write report + done ----
    try_write_report(report, out_dir, total_timer);

    log_info("GENMESH_I0000", "genmesh completed successfully", {
        {"triangles", std::to_string(report.stats.triangle_count)},
        {"vertices", std::to_string(report.stats.vertex_count)},
        {"active_voxels", std::to_string(report.stats.active_voxel_count)},
    });

    return static_cast<int>(ExitCode::Success);
}
