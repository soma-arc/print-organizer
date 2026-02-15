#include "genmesh/bricks_data.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <cstdint>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

namespace genmesh {

// ---------- helpers ----------

static void add_error(BricksDataResult& r, std::string_view code,
                      const std::string& msg, const std::string& field = "") {
    r.errors.push_back({std::string(code), msg, field});
    log_error(code, msg, field.empty() ? std::vector<KV>{} : std::vector<KV>{{"field", field}});
}

/// Software f16 → f32 conversion (IEEE 754 binary16).
static float half_to_float(uint16_t h) {
    uint32_t sign = (h >> 15) & 0x1;
    uint32_t exp  = (h >> 10) & 0x1F;
    uint32_t mant = h & 0x3FF;

    uint32_t f;
    if (exp == 0) {
        if (mant == 0) {
            // zero
            f = sign << 31;
        } else {
            // denormalized → normalized
            exp = 1;
            while (!(mant & 0x400)) {
                mant <<= 1;
                exp--;
            }
            mant &= 0x3FF;
            f = (sign << 31) | ((exp + 127 - 15) << 23) | (mant << 13);
        }
    } else if (exp == 31) {
        // inf or NaN
        f = (sign << 31) | 0x7F800000 | (mant << 13);
    } else {
        // normalized
        f = (sign << 31) | ((exp + 127 - 15) << 23) | (mant << 13);
    }

    float result;
    std::memcpy(&result, &f, sizeof(float));
    return result;
}

/// CRC32 (ISO 3309 / zlib compatible) for verification.
static uint32_t crc32_calc(const uint8_t* data, size_t len) {
    // Standard CRC32 table-based implementation
    static uint32_t table[256] = {0};
    static bool table_init = false;
    if (!table_init) {
        for (uint32_t i = 0; i < 256; ++i) {
            uint32_t crc = i;
            for (int j = 0; j < 8; ++j) {
                crc = (crc >> 1) ^ ((crc & 1) ? 0xEDB88320u : 0);
            }
            table[i] = crc;
        }
        table_init = true;
    }

    uint32_t crc = 0xFFFFFFFF;
    for (size_t i = 0; i < len; ++i) {
        crc = table[(crc ^ data[i]) & 0xFF] ^ (crc >> 8);
    }
    return crc ^ 0xFFFFFFFF;
}

static std::string to_hex8(uint32_t val) {
    char buf[9];
    snprintf(buf, sizeof(buf), "%08x", val);
    return std::string(buf);
}

static uint32_t hex_to_u32(const std::string& hex) {
    return static_cast<uint32_t>(std::stoul(hex, nullptr, 16));
}

// ---------- load ----------

BricksDataResult load_bricks_bin(const std::string& bin_path,
                                 const BricksIndex& index,
                                 const Manifest& manifest) {
    BricksDataResult result;

    // Open binary file
    std::ifstream ifs(bin_path, std::ios::binary | std::ios::ate);
    if (!ifs.is_open()) {
        add_error(result, E2001, "Cannot open bricks.bin: " + bin_path);
        result.exit_code = ExitCode::IoError;
        return result;
    }

    const int64_t file_size = static_cast<int64_t>(ifs.tellg());
    ifs.seekg(0, std::ios::beg);

    const int B = index.brick_size;
    const int64_t voxels_per_brick = static_cast<int64_t>(B) * B * B;
    const bool is_f16 = (index.dtype == "f16");
    const int sizeof_dtype = is_f16 ? 2 : 4;

    result.bricks.reserve(index.bricks.size());

    for (size_t bi = 0; bi < index.bricks.size(); ++bi) {
        const auto& entry = index.bricks[bi];
        std::string prefix = "bricks[" + std::to_string(bi) + "]";

        // --- range check (§5.6) ---
        if (entry.offset_bytes + entry.payload_bytes > file_size) {
            add_error(result, E1105,
                      prefix + " offset_bytes(" + std::to_string(entry.offset_bytes) +
                      ") + payload_bytes(" + std::to_string(entry.payload_bytes) +
                      ") exceeds file size(" + std::to_string(file_size) + ")",
                      "bricks.bin");
            continue;
        }

        // Read raw payload
        std::vector<uint8_t> raw(static_cast<size_t>(entry.payload_bytes));
        ifs.seekg(entry.offset_bytes, std::ios::beg);
        ifs.read(reinterpret_cast<char*>(raw.data()), entry.payload_bytes);
        if (!ifs) {
            add_error(result, E2001,
                      prefix + " read failed at offset " + std::to_string(entry.offset_bytes),
                      "bricks.bin");
            continue;
        }

        // --- CRC32 check (§5.6, optional) ---
        if (entry.crc32.has_value()) {
            uint32_t computed = crc32_calc(raw.data(), raw.size());
            uint32_t expected = hex_to_u32(entry.crc32.value());
            if (computed != expected) {
                add_error(result, E1106,
                          prefix + " CRC32 mismatch: computed=" + to_hex8(computed) +
                          " expected=" + entry.crc32.value(),
                          "bricks.bin");
                continue;
            }
        }

        // --- Convert to float array ---
        BrickData bd;
        bd.bx = entry.bx;
        bd.by = entry.by;
        bd.bz = entry.bz;
        bd.values.resize(static_cast<size_t>(voxels_per_brick));

        if (is_f16) {
            for (int64_t i = 0; i < voxels_per_brick; ++i) {
                uint16_t h;
                std::memcpy(&h, raw.data() + i * 2, 2);
                bd.values[static_cast<size_t>(i)] = half_to_float(h);
            }
        } else {
            // f32: direct memcpy (little-endian assumed)
            std::memcpy(bd.values.data(), raw.data(),
                        static_cast<size_t>(voxels_per_brick) * sizeof(float));
        }

        result.bricks.push_back(std::move(bd));
    }

    // --- final result ---
    if (result.errors.empty()) {
        result.ok = true;
        result.exit_code = ExitCode::Success;
    } else {
        result.ok = false;
        result.exit_code = ExitCode::ValidationFailure;
    }

    return result;
}

}  // namespace genmesh
