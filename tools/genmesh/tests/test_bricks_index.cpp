// T2.1 Bricks index parse + validation tests
#include <cassert>
#include <fstream>
#include <iostream>
#include <string>

#include <nlohmann/json.hpp>
#include "genmesh/bricks_index.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"
#include "genmesh/manifest.h"

static std::string fixture_dir() {
    std::string file = __FILE__;
    auto pos = file.rfind("tests");
    return file.substr(0, pos) + "tests/fixtures/";
}

// Build a manifest matching our valid fixture
static genmesh::Manifest make_manifest() {
    genmesh::Manifest m;
    m.version = 1;
    m.brick_size = 64;
    m.dtype = "f32";
    m.axis_order = "x-fastest";
    m.dims = {64, 64, 64};
    m.voxel_size = 1.0f;
    m.aabb_size = {64.0f, 64.0f, 64.0f};
    m.aabb_min = {0.0f, 0.0f, 0.0f};
    m.half_width_voxels = 3;
    m.background_value_mm = 1000.0f;
    m.handedness = "right";
    m.up_axis = "Y";
    m.front_axis = "+Z";
    m.units = "mm";
    m.sample_at = "voxel_center";
    m.distance_sign = "negative_inside_positive_outside";
    m.iso = 0.0f;
    m.adaptivity = 0.0f;
    return m;
}

static std::string write_temp_json(const nlohmann::json& j, const std::string& name) {
    std::ofstream ofs(name);
    ofs << j.dump(2);
    ofs.close();
    return name;
}

static nlohmann::json valid_base() {
    std::ifstream ifs(fixture_dir() + "valid_bricks_index.json");
    return nlohmann::json::parse(ifs);
}

static bool has_error_code(const genmesh::BricksIndexResult& r, std::string_view code) {
    for (const auto& e : r.errors) {
        if (e.code == code) return true;
    }
    return false;
}

void test_valid_index() {
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(fixture_dir() + "valid_bricks_index.json", m);
    assert(r.ok);
    assert(r.errors.empty());
    assert(r.index.version == 1);
    assert(r.index.brick_size == 64);
    assert(r.index.dtype == "f32");
    assert(r.index.axis_order == "x-fastest");
    assert(r.index.dims[0] == 64);
    assert(r.index.bricks.size() == 1);
    assert(r.index.bricks[0].bx == 0);
    assert(r.index.bricks[0].offset_bytes == 0);
    assert(r.index.bricks[0].payload_bytes == 1048576);  // 64^3 * 4
    assert(r.index.bricks[0].encoding == "raw");
    std::cout << "  PASS: test_valid_index\n";
}

void test_missing_file() {
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index("nonexistent.json", m);
    assert(!r.ok);
    assert(r.exit_code == genmesh::ExitCode::IoError);
    assert(has_error_code(r, genmesh::E2003));
    std::cout << "  PASS: test_missing_file\n";
}

void test_invalid_json() {
    std::ofstream ofs("_bad_bi.json");
    ofs << "{ broken json";
    ofs.close();
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index("_bad_bi.json", m);
    assert(!r.ok);
    std::remove("_bad_bi.json");
    std::cout << "  PASS: test_invalid_json\n";
}

void test_brick_size_mismatch() {
    auto j = valid_base();
    j["brick_size"] = 32;
    // adjust payload_bytes for 32^3*4=131072
    j["bricks"][0]["payload_bytes"] = 131072;
    auto path = write_temp_json(j, "_bi_bs.json");
    auto m = make_manifest();  // brick_size=64
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1101));
    std::remove(path.c_str());
    std::cout << "  PASS: test_brick_size_mismatch\n";
}

void test_dtype_mismatch() {
    auto j = valid_base();
    j["dtype"] = "f16";
    auto path = write_temp_json(j, "_bi_dt.json");
    auto m = make_manifest();  // dtype=f32
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1101));
    std::remove(path.c_str());
    std::cout << "  PASS: test_dtype_mismatch\n";
}

void test_dims_mismatch() {
    auto j = valid_base();
    j["dims"] = {128, 64, 64};
    auto path = write_temp_json(j, "_bi_dims.json");
    auto m = make_manifest();  // dims=[64,64,64]
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1101));
    std::remove(path.c_str());
    std::cout << "  PASS: test_dims_mismatch\n";
}

void test_duplicate_brick() {
    auto j = valid_base();
    // Add a second brick with same (0,0,0)
    j["bricks"].push_back({
        {"bx", 0}, {"by", 0}, {"bz", 0},
        {"offset_bytes", 1048576},
        {"payload_bytes", 1048576},
        {"encoding", "raw"}
    });
    auto path = write_temp_json(j, "_bi_dup.json");
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1102));
    std::remove(path.c_str());
    std::cout << "  PASS: test_duplicate_brick\n";
}

void test_brick_out_of_range() {
    auto j = valid_base();
    // dims=64, B=64 → max_bx=0. Set bx=1 → out of range
    j["bricks"][0]["bx"] = 1;
    auto path = write_temp_json(j, "_bi_oor.json");
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1103));
    std::remove(path.c_str());
    std::cout << "  PASS: test_brick_out_of_range\n";
}

void test_payload_bytes_mismatch() {
    auto j = valid_base();
    j["bricks"][0]["payload_bytes"] = 999;  // should be 64^3*4=1048576
    auto path = write_temp_json(j, "_bi_pay.json");
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1104));
    std::remove(path.c_str());
    std::cout << "  PASS: test_payload_bytes_mismatch\n";
}

void test_invalid_encoding() {
    auto j = valid_base();
    j["bricks"][0]["encoding"] = "zstd";
    auto path = write_temp_json(j, "_bi_enc.json");
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(path, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1101));
    std::remove(path.c_str());
    std::cout << "  PASS: test_invalid_encoding\n";
}

void test_optional_crc32() {
    auto j = valid_base();
    j["bricks"][0]["crc32"] = "abcd1234";
    auto path = write_temp_json(j, "_bi_crc.json");
    auto m = make_manifest();
    auto r = genmesh::load_bricks_index(path, m);
    assert(r.ok);
    assert(r.index.bricks[0].crc32.has_value());
    assert(r.index.bricks[0].crc32.value() == "abcd1234");
    std::remove(path.c_str());
    std::cout << "  PASS: test_optional_crc32\n";
}

int main() {
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T2.1 Bricks index tests ===\n";

    test_valid_index();
    test_missing_file();
    test_invalid_json();
    test_brick_size_mismatch();
    test_dtype_mismatch();
    test_dims_mismatch();
    test_duplicate_brick();
    test_brick_out_of_range();
    test_payload_bytes_mismatch();
    test_invalid_encoding();
    test_optional_crc32();

    std::cout << "=== All T2.1 tests passed ===\n";
    return 0;
}
