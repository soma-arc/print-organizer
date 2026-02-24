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
#include "genmesh/vdb_builder.h"

namespace fs = std::filesystem;

int main(int argc, char* argv[]) {
    using namespace genmesh;

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

    // ---- 2. Prepare output directory ----
    auto out_res = prepare_output_dir(args.out_dir, args.write_stl, args.write_vdb, args.force);
    if (!out_res.ok) {
        return static_cast<int>(out_res.exit_code);
    }

    // ---- 3. Acquire manifest + brick data ----
    Manifest manifest;
    std::vector<BrickData> bricks;

    if (!args.debug_generate.empty()) {
        // debug-generate path: create data internally
        log_info("GENMESH_I0000", "Using debug-generate mode", {
            {"shape", args.debug_generate},
        });

        auto dg = debug_generate(args.debug_generate);
        if (!dg.ok) {
            log_error(E9001, dg.error_msg);
            return static_cast<int>(dg.exit_code);
        }
        manifest = std::move(dg.manifest);
        bricks = std::move(dg.bricks);
    } else {
        // Normal path: load from files
        // 3a. Load manifest
        auto mr = load_manifest(args.manifest_path);
        if (!mr.ok) {
            for (const auto& e : mr.errors) {
                log_error(e.code, e.message, {{"field", e.field}});
            }
            return static_cast<int>(mr.exit_code);
        }
        manifest = std::move(mr.manifest);

        // Apply CLI overrides
        if (args.iso.has_value()) manifest.iso = args.iso.value();
        if (args.adaptivity.has_value()) manifest.adaptivity = args.adaptivity.value();

        // 3b. Load bricks index
        auto idx_path = (fs::path(args.in_dir) / "bricks.index.json").string();
        auto ir = load_bricks_index(idx_path, manifest);
        if (!ir.ok) {
            for (const auto& e : ir.errors) {
                log_error(e.code, e.message, {{"field", e.field}});
            }
            return static_cast<int>(ir.exit_code);
        }

        // 3c. Load bricks binary data
        auto bin_path = (fs::path(args.in_dir) / "bricks.bin").string();
        auto br = load_bricks_bin(bin_path, ir.index, manifest);
        if (!br.ok) {
            for (const auto& e : br.errors) {
                log_error(e.code, e.message, {{"field", e.field}});
            }
            return static_cast<int>(br.exit_code);
        }
        bricks = std::move(br.bricks);
    }

    // ---- 4. OpenVDB init + build grid ----
    if (!vdb_init()) {
        return static_cast<int>(ExitCode::EnvironmentError);
    }

    auto vdb_res = build_vdb(manifest, bricks);
    if (!vdb_res.ok) {
        return static_cast<int>(vdb_res.exit_code);
    }

    // ---- 5. Mesh extraction ----
    double iso = static_cast<double>(manifest.iso);
    double adaptivity = static_cast<double>(manifest.adaptivity);

    auto mesh_res = extract_mesh(vdb_res.grid, iso, adaptivity);
    if (!mesh_res.ok) {
        return static_cast<int>(mesh_res.exit_code);
    }

    if (mesh_res.mesh.triangles.empty()) {
        log_warn(W5002, "Mesh has zero triangles");
    }

    // ---- 6. Write outputs ----
    fs::path out_dir(args.out_dir);

    // 6a. STL
    if (args.write_stl) {
        auto stl_res = write_stl(out_dir / "mesh.stl", mesh_res.mesh);
        if (!stl_res.ok) {
            return static_cast<int>(stl_res.exit_code);
        }
    }

    // 6b. VDB
    if (args.write_vdb) {
        auto vdb_wr = write_vdb(out_dir / "volume.vdb", vdb_res.grid);
        if (!vdb_wr.ok) {
            return static_cast<int>(vdb_wr.exit_code);
        }
    }

    // ---- 7. Done ----
    log_info("GENMESH_I0000", "genmesh completed successfully", {
        {"triangles", std::to_string(mesh_res.mesh.triangles.size())},
        {"vertices", std::to_string(mesh_res.mesh.points.size())},
        {"active_voxels", std::to_string(vdb_res.active_voxel_count)},
    });

    return static_cast<int>(ExitCode::Success);
}
