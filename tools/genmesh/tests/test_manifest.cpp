// T1.2 Manifest parse + validation tests
#include <cassert>
#include <cmath>
#include <fstream>
#include <iostream>
#include <string>

#include <nlohmann/json.hpp>
#include "genmesh/manifest.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

// Path to test fixtures (set relative to build dir)
// Tests are run from the build directory; fixtures are at ../tests/fixtures/
static std::string fixture_dir() {
    // __FILE__ gives us the absolute path to this source file
    std::string file = __FILE__;
    // go up to tests/ then into fixtures/
    auto pos = file.rfind("tests");
    return file.substr(0, pos) + "tests/fixtures/";
}

// Helper: write a temporary JSON file and return its path
static std::string write_temp_json(const nlohmann::json& j, const std::string& name) {
    std::string path = name;
    std::ofstream ofs(path);
    ofs << j.dump(2);
    ofs.close();
    return path;
}

// Helper: load the valid manifest fixture as a base
static nlohmann::json valid_base() {
    std::ifstream ifs(fixture_dir() + "valid_manifest.json");
    return nlohmann::json::parse(ifs);
}

// Helper: check if any error has the given code
static bool has_error_code(const genmesh::ManifestResult& r, std::string_view code) {
    for (const auto& e : r.errors) {
        if (e.code == code) return true;
    }
    return false;
}

void test_valid_manifest() {
    auto r = genmesh::load_manifest(fixture_dir() + "valid_manifest.json");
    assert(r.ok);
    assert(r.errors.empty());
    assert(r.manifest.version == 1);
    assert(r.manifest.handedness == "right");
    assert(r.manifest.up_axis == "Y");
    assert(r.manifest.front_axis == "+Z");
    assert(r.manifest.units == "mm");
    assert(r.manifest.voxel_size == 1.0f);
    assert(r.manifest.dims[0] == 64);
    assert(r.manifest.brick_size == 64);
    assert(r.manifest.dtype == "f32");
    assert(r.manifest.background_value_mm == 1000.0f);
    assert(r.manifest.iso == 0.0f);
    assert(r.manifest.adaptivity == 0.0f);
    assert(r.manifest.half_width_voxels == 3);
    std::cout << "  PASS: test_valid_manifest\n";
}

void test_missing_file() {
    auto r = genmesh::load_manifest("nonexistent.json");
    assert(!r.ok);
    assert(r.exit_code == genmesh::ExitCode::IoError);
    assert(has_error_code(r, genmesh::E2002));
    std::cout << "  PASS: test_missing_file\n";
}

void test_invalid_json() {
    std::ofstream ofs("_bad.json");
    ofs << "{ not valid json";
    ofs.close();
    auto r = genmesh::load_manifest("_bad.json");
    assert(!r.ok);
    std::remove("_bad.json");
    std::cout << "  PASS: test_invalid_json\n";
}

void test_missing_required_field() {
    auto j = valid_base();
    j.erase("dims");
    auto path = write_temp_json(j, "_no_dims.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1001));
    std::remove(path.c_str());
    std::cout << "  PASS: test_missing_required_field\n";
}

void test_wrong_coordinate_system() {
    auto j = valid_base();
    j["coordinate_system"]["handedness"] = "left";
    auto path = write_temp_json(j, "_bad_cs.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1003));
    std::remove(path.c_str());
    std::cout << "  PASS: test_wrong_coordinate_system\n";
}

void test_wrong_distance_sign() {
    auto j = valid_base();
    j["distance_sign"] = "positive_inside_negative_outside";
    auto path = write_temp_json(j, "_bad_ds.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1004));
    std::remove(path.c_str());
    std::cout << "  PASS: test_wrong_distance_sign\n";
}

void test_adaptivity_out_of_range() {
    auto j = valid_base();
    j["adaptivity"] = 1.5;
    auto path = write_temp_json(j, "_bad_adapt.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1005));
    std::remove(path.c_str());
    std::cout << "  PASS: test_adaptivity_out_of_range\n";
}

void test_invalid_brick_size() {
    auto j = valid_base();
    j["brick"]["size"] = 48;
    auto path = write_temp_json(j, "_bad_brick.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1006));
    std::remove(path.c_str());
    std::cout << "  PASS: test_invalid_brick_size\n";
}

void test_aabb_size_mismatch() {
    auto j = valid_base();
    j["aabb_size"] = {100.0, 64.0, 64.0};  // dims=64, voxel=1.0 → expect 64.0
    auto path = write_temp_json(j, "_bad_aabb.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1002));
    std::remove(path.c_str());
    std::cout << "  PASS: test_aabb_size_mismatch\n";
}

void test_background_too_small() {
    auto j = valid_base();
    // half_width=3, voxel=1.0 → band=3.0, background must be >= 3.0
    j["background_value_mm"] = 2.0;
    auto path = write_temp_json(j, "_bad_bg.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1007));
    std::remove(path.c_str());
    std::cout << "  PASS: test_background_too_small\n";
}

void test_invalid_dtype() {
    auto j = valid_base();
    j["dtype"] = "f64";
    auto path = write_temp_json(j, "_bad_dtype.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1002));
    std::remove(path.c_str());
    std::cout << "  PASS: test_invalid_dtype\n";
}

void test_negative_voxel_size() {
    auto j = valid_base();
    j["voxel_size"] = -1.0;
    auto path = write_temp_json(j, "_bad_vs.json");
    auto r = genmesh::load_manifest(path);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1002));
    std::remove(path.c_str());
    std::cout << "  PASS: test_negative_voxel_size\n";
}

int main() {
    // suppress log noise during tests
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T1.2 Manifest tests ===\n";

    test_valid_manifest();
    test_missing_file();
    test_invalid_json();
    test_missing_required_field();
    test_wrong_coordinate_system();
    test_wrong_distance_sign();
    test_adaptivity_out_of_range();
    test_invalid_brick_size();
    test_aabb_size_mismatch();
    test_background_too_small();
    test_invalid_dtype();
    test_negative_voxel_size();

    std::cout << "=== All T1.2 tests passed ===\n";
    return 0;
}
