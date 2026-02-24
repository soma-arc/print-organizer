#include "genmesh/report.h"

#include <nlohmann/json.hpp>

#include <cassert>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <thread>

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

// ---------- helpers ----------

static fs::path make_temp_dir(const std::string& prefix) {
    auto p = fs::temp_directory_path() / (prefix + "_genmesh_report_test");
    fs::create_directories(p);
    return p;
}

static genmesh::Report make_success_report() {
    genmesh::Report r;
    r.schema_version = 1;
    r.status = "success";
    r.stage = genmesh::Stage::Write;
    r.started_at_utc = "2026-02-24T12:00:00Z";
    r.ended_at_utc = "2026-02-24T12:00:01Z";

    r.inputs.manifest_path = "project.json";
    r.inputs.in_dir = "data/";
    r.inputs.dtype = "f32";
    r.inputs.brick_size = 64;
    r.inputs.dims = {64, 64, 64};
    r.inputs.voxel_size = 1.0f;

    r.timing_ms.total = 1234.5;
    r.timing_ms.validate = 10.0;
    r.timing_ms.read = 20.0;
    r.timing_ms.vdb_build = 300.0;
    r.timing_ms.meshing = 800.0;
    r.timing_ms.write = 104.5;

    r.stats.aabb_min = {0.0f, 0.0f, 0.0f};
    r.stats.aabb_max = {64.0f, 64.0f, 64.0f};
    r.stats.brick_count = 1;
    r.stats.triangle_count = 24672;
    r.stats.quad_count = 12336;
    r.stats.vertex_count = 12338;
    r.stats.degenerate_count = 0;
    r.stats.active_voxel_count = 262144;

    return r;
}

// ---------- tests ----------

void test_report_to_json_required_fields() {
    auto r = make_success_report();
    auto j = genmesh::report_to_json(r);

    ASSERT(j["schema_version"] == 1);
    ASSERT(j["status"] == "success");
    ASSERT(j["stage"] == "write");
    ASSERT(j["started_at_utc"] == "2026-02-24T12:00:00Z");
    ASSERT(j["ended_at_utc"] == "2026-02-24T12:00:01Z");
    ASSERT(j.contains("inputs"));
    ASSERT(j.contains("timing_ms"));
    ASSERT(j.contains("stats"));
    ASSERT(j.contains("warnings"));
    ASSERT(j.contains("errors"));
}

void test_report_to_json_inputs() {
    auto r = make_success_report();
    auto j = genmesh::report_to_json(r);
    auto inp = j["inputs"];

    ASSERT(inp["manifest_path"] == "project.json");
    ASSERT(inp["in_dir"] == "data/");
    ASSERT(inp["dtype"] == "f32");
    ASSERT(inp["brick_size"] == 64);
    ASSERT(inp["dims"][0] == 64);
    ASSERT(inp["dims"][1] == 64);
    ASSERT(inp["dims"][2] == 64);
    ASSERT(inp["voxel_size"] == 1.0);
}

void test_report_to_json_timing() {
    auto r = make_success_report();
    auto j = genmesh::report_to_json(r);
    auto t = j["timing_ms"];

    ASSERT(t["total"] == 1234.5);
    ASSERT(t["validate"] == 10.0);
    ASSERT(t["read"] == 20.0);
    ASSERT(t["vdb_build"] == 300.0);
    ASSERT(t["meshing"] == 800.0);
    ASSERT(t["write"] == 104.5);
}

void test_report_to_json_timing_omits_unmeasured() {
    auto r = make_success_report();
    r.timing_ms.read = -1.0;  // not measured
    r.timing_ms.write = -1.0;
    auto j = genmesh::report_to_json(r);
    auto t = j["timing_ms"];

    ASSERT(t.contains("total"));
    ASSERT(t.contains("validate"));
    ASSERT(!t.contains("read"));
    ASSERT(t.contains("vdb_build"));
    ASSERT(t.contains("meshing"));
    ASSERT(!t.contains("write"));
}

void test_report_to_json_stats() {
    auto r = make_success_report();
    auto j = genmesh::report_to_json(r);
    auto s = j["stats"];

    ASSERT(s["aabb_min"][0] == 0.0f);
    ASSERT(s["aabb_max"][0] == 64.0f);
    ASSERT(s["brick_count"] == 1);
    ASSERT(s["triangle_count"] == 24672);
    ASSERT(s["quad_count"] == 12336);
    ASSERT(s["vertex_count"] == 12338);
    ASSERT(s["degenerate_count"] == 0);
    ASSERT(s["active_voxel_count"] == 262144);
    ASSERT(!s.contains("mesh_aabb_min"));
    ASSERT(!s.contains("mesh_aabb_max"));
}

void test_report_to_json_mesh_aabb() {
    auto r = make_success_report();
    r.stats.has_mesh_aabb = true;
    r.stats.mesh_aabb_min = {1.0f, 2.0f, 3.0f};
    r.stats.mesh_aabb_max = {10.0f, 20.0f, 30.0f};
    auto j = genmesh::report_to_json(r);
    auto s = j["stats"];

    ASSERT(s.contains("mesh_aabb_min"));
    ASSERT(s.contains("mesh_aabb_max"));
    ASSERT(s["mesh_aabb_min"][0] == 1.0f);
    ASSERT(s["mesh_aabb_max"][2] == 30.0f);
}

void test_report_to_json_warnings() {
    auto r = make_success_report();
    r.warnings.push_back({"GENMESH_W5001", "Degenerate triangles", "meshing",
                           "", {{"count", 5}}, ""});
    auto j = genmesh::report_to_json(r);

    ASSERT(j["warnings"].size() == 1);
    ASSERT(j["warnings"][0]["code"] == "GENMESH_W5001");
    ASSERT(j["warnings"][0]["message"] == "Degenerate triangles");
    ASSERT(j["warnings"][0]["kind"] == "meshing");
    ASSERT(j["warnings"][0]["context"]["count"] == 5);
}

void test_report_to_json_errors_with_kind() {
    auto r = make_success_report();
    r.status = "failure";
    r.stage = genmesh::Stage::Validate;
    r.errors.push_back({"GENMESH_E1001", "missing field: dims", "validation",
                         "Check manifest", {{"field", "dims"}}, "parse error"});
    auto j = genmesh::report_to_json(r);

    ASSERT(j["status"] == "failure");
    ASSERT(j["stage"] == "validate");
    ASSERT(j["errors"].size() == 1);
    ASSERT(j["errors"][0]["code"] == "GENMESH_E1001");
    ASSERT(j["errors"][0]["kind"] == "validation");
    ASSERT(j["errors"][0]["hint"] == "Check manifest");
    ASSERT(j["errors"][0]["caused_by"] == "parse error");
    ASSERT(j["errors"][0]["context"]["field"] == "dims");
}

void test_report_to_json_progress() {
    auto r = make_success_report();
    r.status = "failure";
    r.has_progress = true;
    r.progress.stage = genmesh::Stage::Read;
    r.progress.percent = 50.0;
    r.progress.detail = "reading bricks.bin";
    auto j = genmesh::report_to_json(r);

    ASSERT(j.contains("progress"));
    ASSERT(j["progress"]["stage"] == "read");
    ASSERT(j["progress"]["percent"] == 50.0);
    ASSERT(j["progress"]["detail"] == "reading bricks.bin");
}

void test_report_to_json_no_progress_when_not_set() {
    auto r = make_success_report();
    r.has_progress = false;
    auto j = genmesh::report_to_json(r);

    ASSERT(!j.contains("progress"));
}

void test_write_report_creates_file() {
    auto r = make_success_report();
    auto dir = make_temp_dir("report_write");
    auto path = dir / "report.json";

    auto wr = genmesh::write_report(path, r);
    ASSERT(wr.ok);
    ASSERT(fs::exists(path));
    ASSERT(fs::file_size(path) > 0);

    // Read back and verify it's valid JSON
    std::ifstream ifs(path);
    auto j = nlohmann::json::parse(ifs);
    ifs.close();
    ASSERT(j["schema_version"] == 1);
    ASSERT(j["status"] == "success");

    fs::remove_all(dir);
}

void test_write_report_no_temp_file_remains() {
    auto r = make_success_report();
    auto dir = make_temp_dir("report_notmp");
    auto path = dir / "report.json";
    auto tmp_path = path;
    tmp_path += ".tmp";

    auto wr = genmesh::write_report(path, r);
    ASSERT(wr.ok);
    ASSERT(fs::exists(path));
    ASSERT(!fs::exists(tmp_path));

    fs::remove_all(dir);
}

void test_utc_now_iso8601_format() {
    auto ts = genmesh::utc_now_iso8601();
    // Should match pattern: YYYY-MM-DDTHH:MM:SSZ
    ASSERT(ts.size() == 20);
    ASSERT(ts[4] == '-');
    ASSERT(ts[7] == '-');
    ASSERT(ts[10] == 'T');
    ASSERT(ts[13] == ':');
    ASSERT(ts[16] == ':');
    ASSERT(ts[19] == 'Z');
}

void test_scoped_timer_measures_time() {
    genmesh::ScopedTimer timer;
    // Sleep briefly
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
    double ms = timer.elapsed_ms();
    ASSERT(ms >= 5.0);   // at least some time passed
    ASSERT(ms < 5000.0);  // not absurdly long
}

void test_stage_to_string() {
    ASSERT(std::string(genmesh::stage_to_string(genmesh::Stage::Validate)) == "validate");
    ASSERT(std::string(genmesh::stage_to_string(genmesh::Stage::Read)) == "read");
    ASSERT(std::string(genmesh::stage_to_string(genmesh::Stage::VdbBuild)) == "vdb_build");
    ASSERT(std::string(genmesh::stage_to_string(genmesh::Stage::Meshing)) == "meshing");
    ASSERT(std::string(genmesh::stage_to_string(genmesh::Stage::Write)) == "write");
}

// ---------- main ----------

int main() {
    std::cout << "=== test_report ===\n";

    // Serialization
    RUN(test_report_to_json_required_fields);
    RUN(test_report_to_json_inputs);
    RUN(test_report_to_json_timing);
    RUN(test_report_to_json_timing_omits_unmeasured);
    RUN(test_report_to_json_stats);
    RUN(test_report_to_json_mesh_aabb);
    RUN(test_report_to_json_warnings);
    RUN(test_report_to_json_errors_with_kind);
    RUN(test_report_to_json_progress);
    RUN(test_report_to_json_no_progress_when_not_set);

    // File I/O
    RUN(test_write_report_creates_file);
    RUN(test_write_report_no_temp_file_remains);

    // Utilities
    RUN(test_utc_now_iso8601_format);
    RUN(test_scoped_timer_measures_time);
    RUN(test_stage_to_string);

    std::cout << "\n" << tests_passed << "/" << tests_run << " passed\n";
    return (tests_passed == tests_run) ? 0 : 1;
}
