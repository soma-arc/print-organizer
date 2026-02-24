#pragma once

#include <array>
#include <chrono>
#include <cstdint>
#include <filesystem>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

#include "genmesh/exit_code.h"

namespace genmesh {

/// Diagnostic entry (shared by warnings and errors).
struct Diagnostic {
    std::string code;     // GENMESH_[E|W]NNNN
    std::string message;
    std::string kind;     // "validation"|"io"|"env"|"vdb"|"meshing"|"unexpected"
    std::string hint;     // optional user hint
    nlohmann::json context;  // optional machine-readable context
    std::string caused_by;   // optional (errors only)
};

/// Timing measurements in milliseconds.
struct TimingMs {
    double total = 0.0;
    double validate = -1.0;   // negative = not measured
    double read = -1.0;
    double vdb_build = -1.0;
    double meshing = -1.0;
    double write = -1.0;
};

/// Mesh / grid statistics.
struct Stats {
    std::array<float, 3> aabb_min = {};
    std::array<float, 3> aabb_max = {};
    int64_t brick_count = 0;
    int64_t triangle_count = 0;
    int64_t quad_count = 0;
    int64_t vertex_count = 0;
    int64_t degenerate_count = 0;

    // optional
    bool has_mesh_aabb = false;
    std::array<float, 3> mesh_aabb_min = {};
    std::array<float, 3> mesh_aabb_max = {};
    int64_t active_voxel_count = -1;  // negative = not available
};

/// Input information recorded in the report.
struct ReportInputs {
    std::string manifest_path;
    std::string in_dir;
    std::string bricks_path;  // optional
    std::string dtype;
    int brick_size = 0;
    std::array<int, 3> dims = {};
    float voxel_size = 0.0f;
};

/// Pipeline stage identifiers.
enum class Stage {
    Validate,
    Read,
    VdbBuild,
    Meshing,
    Write,
};

/// Convert Stage to schema string.
const char* stage_to_string(Stage s);

/// Progress info (for failure reports).
struct Progress {
    Stage stage = Stage::Validate;
    double percent = -1.0;  // negative = not available
    std::string detail;
};

/// Complete report structure matching report.v1.schema.json.
struct Report {
    int schema_version = 1;
    std::string status;  // "success" | "failure"
    Stage stage = Stage::Write;
    std::string started_at_utc;
    std::string ended_at_utc;
    ReportInputs inputs;
    TimingMs timing_ms;
    Stats stats;
    std::vector<Diagnostic> warnings;
    std::vector<Diagnostic> errors;
    Progress progress;  // used on failure
    bool has_progress = false;
};

/// Serialize report to JSON.
nlohmann::json report_to_json(const Report& report);

/// Result of report write.
struct ReportWriteResult {
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::string error_code;
    std::string error_msg;
};

/// Write report.json to the given path.
/// Uses temp + rename for atomic write.
ReportWriteResult write_report(const std::filesystem::path& path,
                               const Report& report);

/// Get current UTC time as ISO 8601 string (e.g. "2026-02-14T01:30:45Z").
std::string utc_now_iso8601();

/// Scoped timer helper.
class ScopedTimer {
public:
    ScopedTimer() : start_(std::chrono::steady_clock::now()) {}

    /// Elapsed time in milliseconds since construction.
    double elapsed_ms() const {
        auto now = std::chrono::steady_clock::now();
        return std::chrono::duration<double, std::milli>(now - start_).count();
    }

private:
    std::chrono::steady_clock::time_point start_;
};

}  // namespace genmesh
