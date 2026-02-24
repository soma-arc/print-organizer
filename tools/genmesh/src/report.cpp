#include "genmesh/report.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <nlohmann/json.hpp>

#include <chrono>
#include <cstring>
#include <ctime>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <sstream>

namespace genmesh {

const char* stage_to_string(Stage s) {
    switch (s) {
        case Stage::Validate: return "validate";
        case Stage::Read:     return "read";
        case Stage::VdbBuild: return "vdb_build";
        case Stage::Meshing:  return "meshing";
        case Stage::Write:    return "write";
    }
    return "validate";
}

std::string utc_now_iso8601() {
    auto now = std::chrono::system_clock::now();
    auto time_t_now = std::chrono::system_clock::to_time_t(now);

    struct tm utc_tm;
#ifdef _WIN32
    gmtime_s(&utc_tm, &time_t_now);
#else
    gmtime_r(&time_t_now, &utc_tm);
#endif

    std::ostringstream oss;
    oss << std::put_time(&utc_tm, "%Y-%m-%dT%H:%M:%S") << "Z";
    return oss.str();
}

static nlohmann::json diagnostic_to_json(const Diagnostic& d) {
    nlohmann::json j;
    j["code"] = d.code;
    j["message"] = d.message;
    if (!d.kind.empty())    j["kind"] = d.kind;
    if (!d.hint.empty())    j["hint"] = d.hint;
    if (!d.context.is_null() && !d.context.empty()) j["context"] = d.context;
    if (!d.caused_by.empty()) j["caused_by"] = d.caused_by;
    return j;
}

nlohmann::json report_to_json(const Report& report) {
    nlohmann::json j;

    j["schema_version"] = report.schema_version;
    j["status"] = report.status;
    j["stage"] = stage_to_string(report.stage);
    j["started_at_utc"] = report.started_at_utc;
    if (!report.ended_at_utc.empty()) {
        j["ended_at_utc"] = report.ended_at_utc;
    }

    // inputs
    {
        nlohmann::json inp;
        inp["manifest_path"] = report.inputs.manifest_path;
        inp["in_dir"] = report.inputs.in_dir;
        if (!report.inputs.bricks_path.empty()) {
            inp["bricks_path"] = report.inputs.bricks_path;
        }
        inp["dtype"] = report.inputs.dtype;
        inp["brick_size"] = report.inputs.brick_size;
        inp["dims"] = {report.inputs.dims[0], report.inputs.dims[1], report.inputs.dims[2]};
        inp["voxel_size"] = report.inputs.voxel_size;
        j["inputs"] = inp;
    }

    // timing_ms
    {
        nlohmann::json t;
        t["total"] = report.timing_ms.total;
        if (report.timing_ms.validate >= 0) t["validate"] = report.timing_ms.validate;
        if (report.timing_ms.read >= 0)     t["read"] = report.timing_ms.read;
        if (report.timing_ms.vdb_build >= 0) t["vdb_build"] = report.timing_ms.vdb_build;
        if (report.timing_ms.meshing >= 0)  t["meshing"] = report.timing_ms.meshing;
        if (report.timing_ms.write >= 0)    t["write"] = report.timing_ms.write;
        j["timing_ms"] = t;
    }

    // stats
    {
        nlohmann::json s;
        s["aabb_min"] = {report.stats.aabb_min[0], report.stats.aabb_min[1], report.stats.aabb_min[2]};
        s["aabb_max"] = {report.stats.aabb_max[0], report.stats.aabb_max[1], report.stats.aabb_max[2]};
        s["brick_count"] = report.stats.brick_count;
        s["triangle_count"] = report.stats.triangle_count;
        s["quad_count"] = report.stats.quad_count;
        s["vertex_count"] = report.stats.vertex_count;
        s["degenerate_count"] = report.stats.degenerate_count;
        if (report.stats.has_mesh_aabb) {
            s["mesh_aabb_min"] = {report.stats.mesh_aabb_min[0], report.stats.mesh_aabb_min[1], report.stats.mesh_aabb_min[2]};
            s["mesh_aabb_max"] = {report.stats.mesh_aabb_max[0], report.stats.mesh_aabb_max[1], report.stats.mesh_aabb_max[2]};
        }
        if (report.stats.active_voxel_count >= 0) {
            s["active_voxel_count"] = report.stats.active_voxel_count;
        }
        j["stats"] = s;
    }

    // warnings
    {
        nlohmann::json w = nlohmann::json::array();
        for (const auto& d : report.warnings) {
            w.push_back(diagnostic_to_json(d));
        }
        j["warnings"] = w;
    }

    // errors
    {
        nlohmann::json e = nlohmann::json::array();
        for (const auto& d : report.errors) {
            e.push_back(diagnostic_to_json(d));
        }
        j["errors"] = e;
    }

    // progress (failure only)
    if (report.has_progress) {
        nlohmann::json p;
        p["stage"] = stage_to_string(report.progress.stage);
        if (report.progress.percent >= 0) {
            p["percent"] = report.progress.percent;
        }
        if (!report.progress.detail.empty()) {
            p["detail"] = report.progress.detail;
        }
        j["progress"] = p;
    }

    return j;
}

ReportWriteResult write_report(const std::filesystem::path& path,
                               const Report& report) {
    ReportWriteResult result;

    auto tmp_path = path;
    tmp_path += ".tmp";

    try {
        auto j = report_to_json(report);
        std::string json_str = j.dump(2);

        std::ofstream ofs(tmp_path, std::ios::binary);
        if (!ofs) {
            result.ok = false;
            result.exit_code = ExitCode::IoError;
            result.error_code = std::string(E2101);
            result.error_msg = "Failed to open temp report file: " + tmp_path.string();
            log_error(E2101, result.error_msg);
            return result;
        }

        ofs.write(json_str.data(), static_cast<std::streamsize>(json_str.size()));
        ofs << "\n";
        ofs.close();

        if (ofs.fail()) {
            result.ok = false;
            result.exit_code = ExitCode::IoError;
            result.error_code = std::string(E2101);
            result.error_msg = "Failed to flush report data";
            log_error(E2101, result.error_msg);
            return result;
        }

        // Atomic rename
        std::error_code ec;
        std::filesystem::rename(tmp_path, path, ec);
        if (ec) {
            result.ok = false;
            result.exit_code = ExitCode::IoError;
            result.error_code = std::string(E2101);
            result.error_msg = "Failed to rename temp report file: " + ec.message();
            log_error(E2101, result.error_msg);
            return result;
        }

        log_info("GENMESH_I0006", "report.json written", {
            {"path", path.string()},
        });

        result.ok = true;

    } catch (const std::exception& e) {
        result.ok = false;
        result.exit_code = ExitCode::IoError;
        result.error_code = std::string(E2101);
        result.error_msg = std::string("Report write failed: ") + e.what();
        log_error(E2101, result.error_msg);

        std::error_code ec;
        std::filesystem::remove(tmp_path, ec);
    }

    return result;
}

}  // namespace genmesh
