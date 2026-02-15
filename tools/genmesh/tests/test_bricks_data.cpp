// T2.2 Bricks.bin read tests
//
// Tests use brick_size=2 (2x2x2 = 8 voxels) for minimal fixtures.
// Binary fixtures are generated programmatically in the test.
#include <cassert>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include "genmesh/bricks_data.h"
#include "genmesh/bricks_index.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"
#include "genmesh/manifest.h"

static genmesh::Manifest make_manifest(int B = 2, const std::string& dtype = "f32") {
    genmesh::Manifest m;
    m.version = 1;
    m.brick_size = B;
    m.dtype = dtype;
    m.axis_order = "x-fastest";
    m.dims = {B, B, B};  // exactly 1 brick
    m.voxel_size = 1.0f;
    m.aabb_size = {(float)B, (float)B, (float)B};
    m.aabb_min = {0, 0, 0};
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

// CRC32 matching bricks_data.cpp implementation
static uint32_t crc32_calc(const uint8_t* data, size_t len) {
    static uint32_t table[256] = {0};
    static bool init = false;
    if (!init) {
        for (uint32_t i = 0; i < 256; ++i) {
            uint32_t c = i;
            for (int j = 0; j < 8; ++j)
                c = (c >> 1) ^ ((c & 1) ? 0xEDB88320u : 0);
            table[i] = c;
        }
        init = true;
    }
    uint32_t crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; ++i)
        crc = table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
    return crc ^ 0xFFFFFFFF;
}

static std::string to_hex8(uint32_t val) {
    char buf[9];
    snprintf(buf, sizeof(buf), "%08x", val);
    return std::string(buf);
}

// Write f32 brick data: 8 floats = 32 bytes for B=2
static void write_f32_bin(const std::string& path, const std::vector<float>& values) {
    std::ofstream ofs(path, std::ios::binary);
    ofs.write(reinterpret_cast<const char*>(values.data()), values.size() * sizeof(float));
}

// f32 → f16 conversion (software, IEEE 754 binary16)
static uint16_t float_to_half(float f) {
    uint32_t bits;
    std::memcpy(&bits, &f, sizeof(float));
    uint32_t sign = (bits >> 31) & 1;
    int32_t  exp  = ((bits >> 23) & 0xFF) - 127;
    uint32_t mant = bits & 0x7FFFFF;

    uint16_t h;
    if (exp > 15) {
        h = (uint16_t)((sign << 15) | 0x7C00);  // inf
    } else if (exp < -14) {
        h = (uint16_t)(sign << 15);  // zero / tiny denorm → 0
    } else {
        h = (uint16_t)((sign << 15) | ((exp + 15) << 10) | (mant >> 13));
    }
    return h;
}

static void write_f16_bin(const std::string& path, const std::vector<float>& values) {
    std::ofstream ofs(path, std::ios::binary);
    for (float v : values) {
        uint16_t h = float_to_half(v);
        ofs.write(reinterpret_cast<const char*>(&h), 2);
    }
}

static bool has_error_code(const genmesh::BricksDataResult& r, std::string_view code) {
    for (const auto& e : r.errors)
        if (e.code == code) return true;
    return false;
}

// --------- Tests ---------

void test_read_f32() {
    // B=2 → 8 voxels → 32 bytes
    std::vector<float> data = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f};
    write_f32_bin("_t22_f32.bin", data);

    auto m = make_manifest(2, "f32");

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.axis_order = "x-fastest";
    idx.dims = {2, 2, 2};
    idx.bricks.push_back({0, 0, 0, 0, 32, "raw", std::nullopt});

    auto r = genmesh::load_bricks_bin("_t22_f32.bin", idx, m);
    assert(r.ok);
    assert(r.bricks.size() == 1);
    assert(r.bricks[0].values.size() == 8);
    for (int i = 0; i < 8; ++i) {
        assert(r.bricks[0].values[i] == data[i]);
    }

    std::remove("_t22_f32.bin");
    std::cout << "  PASS: test_read_f32\n";
}

void test_read_f16() {
    std::vector<float> data = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f};
    write_f16_bin("_t22_f16.bin", data);

    auto m = make_manifest(2, "f16");

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f16";
    idx.axis_order = "x-fastest";
    idx.dims = {2, 2, 2};
    idx.bricks.push_back({0, 0, 0, 0, 16, "raw", std::nullopt});  // 8*2=16 bytes

    auto r = genmesh::load_bricks_bin("_t22_f16.bin", idx, m);
    assert(r.ok);
    assert(r.bricks.size() == 1);
    assert(r.bricks[0].values.size() == 8);
    // f16 has limited precision; for exact powers of 2, should be exact
    for (int i = 0; i < 8; ++i) {
        assert(std::abs(r.bricks[0].values[i] - data[i]) < 0.01f);
    }

    std::remove("_t22_f16.bin");
    std::cout << "  PASS: test_read_f16\n";
}

void test_missing_bin_file() {
    auto m = make_manifest();
    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.dims = {2, 2, 2};

    auto r = genmesh::load_bricks_bin("nonexistent.bin", idx, m);
    assert(!r.ok);
    assert(r.exit_code == genmesh::ExitCode::IoError);
    assert(has_error_code(r, genmesh::E2001));
    std::cout << "  PASS: test_missing_bin_file\n";
}

void test_offset_out_of_range() {
    std::vector<float> data = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f};
    write_f32_bin("_t22_oor.bin", data);

    auto m = make_manifest(2, "f32");

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.dims = {2, 2, 2};
    // offset beyond file size (file is 32 bytes)
    idx.bricks.push_back({0, 0, 0, 100, 32, "raw", std::nullopt});

    auto r = genmesh::load_bricks_bin("_t22_oor.bin", idx, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1105));

    std::remove("_t22_oor.bin");
    std::cout << "  PASS: test_offset_out_of_range\n";
}

void test_crc32_valid() {
    std::vector<float> data = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f};
    write_f32_bin("_t22_crc_ok.bin", data);

    // Compute CRC32 of the raw bytes
    std::vector<uint8_t> raw(32);
    std::memcpy(raw.data(), data.data(), 32);
    uint32_t crc = crc32_calc(raw.data(), raw.size());
    std::string crc_hex = to_hex8(crc);

    auto m = make_manifest(2, "f32");

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.dims = {2, 2, 2};
    idx.bricks.push_back({0, 0, 0, 0, 32, "raw", crc_hex});

    auto r = genmesh::load_bricks_bin("_t22_crc_ok.bin", idx, m);
    assert(r.ok);

    std::remove("_t22_crc_ok.bin");
    std::cout << "  PASS: test_crc32_valid\n";
}

void test_crc32_mismatch() {
    std::vector<float> data = {1.0f, 2.0f, 3.0f, 4.0f, 5.0f, 6.0f, 7.0f, 8.0f};
    write_f32_bin("_t22_crc_bad.bin", data);

    auto m = make_manifest(2, "f32");

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.dims = {2, 2, 2};
    idx.bricks.push_back({0, 0, 0, 0, 32, "raw", "deadbeef"});

    auto r = genmesh::load_bricks_bin("_t22_crc_bad.bin", idx, m);
    assert(!r.ok);
    assert(has_error_code(r, genmesh::E1106));

    std::remove("_t22_crc_bad.bin");
    std::cout << "  PASS: test_crc32_mismatch\n";
}

void test_multiple_bricks() {
    // 2 bricks: each 8 floats = 32 bytes, total 64 bytes
    std::vector<float> data1 = {1, 2, 3, 4, 5, 6, 7, 8};
    std::vector<float> data2 = {10, 20, 30, 40, 50, 60, 70, 80};

    {
        std::ofstream ofs("_t22_multi.bin", std::ios::binary);
        ofs.write(reinterpret_cast<const char*>(data1.data()), 32);
        ofs.write(reinterpret_cast<const char*>(data2.data()), 32);
    }

    auto m = make_manifest(2, "f32");
    m.dims = {4, 2, 2};  // 2 bricks in x

    genmesh::BricksIndex idx;
    idx.version = 1;
    idx.brick_size = 2;
    idx.dtype = "f32";
    idx.dims = {4, 2, 2};
    idx.bricks.push_back({0, 0, 0, 0,  32, "raw", std::nullopt});
    idx.bricks.push_back({1, 0, 0, 32, 32, "raw", std::nullopt});

    auto r = genmesh::load_bricks_bin("_t22_multi.bin", idx, m);
    assert(r.ok);
    assert(r.bricks.size() == 2);
    assert(r.bricks[0].values[0] == 1.0f);
    assert(r.bricks[1].values[0] == 10.0f);

    std::remove("_t22_multi.bin");
    std::cout << "  PASS: test_multiple_bricks\n";
}

int main() {
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T2.2 Bricks.bin read tests ===\n";

    test_read_f32();
    test_read_f16();
    test_missing_bin_file();
    test_offset_out_of_range();
    test_crc32_valid();
    test_crc32_mismatch();
    test_multiple_bricks();

    std::cout << "=== All T2.2 tests passed ===\n";
    return 0;
}
