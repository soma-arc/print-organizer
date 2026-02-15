#include "genmesh/manifest.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <nlohmann/json.hpp>
#include <cmath>
#include <fstream>
#include <sstream>

namespace genmesh {

using json = nlohmann::json;

// ---------- helpers ----------

static void add_error(ManifestResult& r, std::string_view code,
                      const std::string& msg, const std::string& field = "") {
    r.errors.push_back({std::string(code), msg, field});
    log_error(code, msg, field.empty() ? std::vector<KV>{} : std::vector<KV>{{"field", field}});
}

// require a field exists and is the expected type
template <typename T>
static bool require_field(const json& j, const std::string& key, ManifestResult& r) {
    if (!j.contains(key)) {
        add_error(r, E1001, "Missing required field: " + key, key);
        return false;
    }
    // type check via get_to would throw; we catch below
    return true;
}

static bool require_string_const(const json& j, const std::string& key,
                                 const std::string& expected, ManifestResult& r,
                                 std::string_view err_code) {
    if (!j.contains(key)) {
        add_error(r, E1001, "Missing required field: " + key, key);
        return false;
    }
    if (!j[key].is_string() || j[key].get<std::string>() != expected) {
        add_error(r, err_code,
                  key + " must be \"" + expected + "\", got: " + j[key].dump(), key);
        return false;
    }
    return true;
}

// ---------- parse + validate ----------

ManifestResult load_manifest(const std::string& path) {
    ManifestResult result;

    // --- read file ---
    std::ifstream ifs(path);
    if (!ifs.is_open()) {
        add_error(result, E2002, "Cannot open manifest: " + path);
        result.exit_code = ExitCode::IoError;
        return result;
    }

    json j;
    try {
        j = json::parse(ifs);
    } catch (const json::parse_error& e) {
        add_error(result, E2002, std::string("Manifest JSON parse error: ") + e.what());
        result.exit_code = ExitCode::ValidationFailure;
        return result;
    }

    auto& m = result.manifest;

    // --- version ---
    if (require_field<int>(j, "version", result)) {
        m.version = j["version"].get<int>();
        if (m.version != 1) {
            add_error(result, E1001, "Unsupported manifest version: " + std::to_string(m.version), "version");
        }
    }

    // --- coordinate_system ---
    if (j.contains("coordinate_system") && j["coordinate_system"].is_object()) {
        auto& cs = j["coordinate_system"];
        require_string_const(cs, "handedness", "right", result, E1003);
        require_string_const(cs, "up_axis", "Y", result, E1003);
        require_string_const(cs, "front_axis", "+Z", result, E1003);
        if (cs.contains("handedness")) m.handedness = cs["handedness"].get<std::string>();
        if (cs.contains("up_axis"))    m.up_axis    = cs["up_axis"].get<std::string>();
        if (cs.contains("front_axis")) m.front_axis = cs["front_axis"].get<std::string>();
    } else {
        add_error(result, E1001, "Missing required field: coordinate_system", "coordinate_system");
    }

    // --- units ---
    require_string_const(j, "units", "mm", result, E1003);
    if (j.contains("units")) m.units = j["units"].get<std::string>();

    // --- aabb_min ---
    if (j.contains("aabb_min") && j["aabb_min"].is_array() && j["aabb_min"].size() == 3) {
        for (int i = 0; i < 3; ++i) m.aabb_min[i] = j["aabb_min"][i].get<float>();
    } else {
        add_error(result, E1001, "Missing or invalid aabb_min (expected float[3])", "aabb_min");
    }

    // --- aabb_size ---
    if (j.contains("aabb_size") && j["aabb_size"].is_array() && j["aabb_size"].size() == 3) {
        for (int i = 0; i < 3; ++i) {
            m.aabb_size[i] = j["aabb_size"][i].get<float>();
            if (m.aabb_size[i] <= 0) {
                add_error(result, E1002, "aabb_size[" + std::to_string(i) + "] must be > 0", "aabb_size");
            }
        }
    } else {
        add_error(result, E1001, "Missing or invalid aabb_size (expected float[3])", "aabb_size");
    }

    // --- voxel_size ---
    if (j.contains("voxel_size") && j["voxel_size"].is_number()) {
        m.voxel_size = j["voxel_size"].get<float>();
        if (m.voxel_size <= 0) {
            add_error(result, E1002, "voxel_size must be > 0", "voxel_size");
        }
    } else {
        add_error(result, E1001, "Missing or invalid voxel_size", "voxel_size");
    }

    // --- dims ---
    if (j.contains("dims") && j["dims"].is_array() && j["dims"].size() == 3) {
        for (int i = 0; i < 3; ++i) {
            m.dims[i] = j["dims"][i].get<int>();
            if (m.dims[i] <= 0) {
                add_error(result, E1002, "dims[" + std::to_string(i) + "] must be > 0", "dims");
            }
        }
    } else {
        add_error(result, E1001, "Missing or invalid dims (expected int[3])", "dims");
    }

    // --- sample_at ---
    require_string_const(j, "sample_at", "voxel_center", result, E1003);
    if (j.contains("sample_at")) m.sample_at = j["sample_at"].get<std::string>();

    // --- axis_order ---
    require_string_const(j, "axis_order", "x-fastest", result, E1003);
    if (j.contains("axis_order")) m.axis_order = j["axis_order"].get<std::string>();

    // --- distance_sign ---
    require_string_const(j, "distance_sign", "negative_inside_positive_outside", result, E1004);
    if (j.contains("distance_sign")) m.distance_sign = j["distance_sign"].get<std::string>();

    // --- iso ---
    if (j.contains("iso") && j["iso"].is_number()) {
        m.iso = j["iso"].get<float>();
    } else {
        add_error(result, E1001, "Missing or invalid iso", "iso");
    }

    // --- adaptivity ---
    if (j.contains("adaptivity") && j["adaptivity"].is_number()) {
        m.adaptivity = j["adaptivity"].get<float>();
        if (m.adaptivity < 0.0f || m.adaptivity > 1.0f) {
            add_error(result, E1005, "adaptivity must be in [0.0, 1.0], got: " +
                      std::to_string(m.adaptivity), "adaptivity");
        }
    } else {
        add_error(result, E1001, "Missing or invalid adaptivity", "adaptivity");
    }

    // --- narrow_band ---
    if (j.contains("narrow_band") && j["narrow_band"].is_object() &&
        j["narrow_band"].contains("half_width_voxels")) {
        m.half_width_voxels = j["narrow_band"]["half_width_voxels"].get<int>();
        if (m.half_width_voxels < 1) {
            add_error(result, E1002, "narrow_band.half_width_voxels must be >= 1",
                      "narrow_band.half_width_voxels");
        }
    } else {
        add_error(result, E1001, "Missing required field: narrow_band.half_width_voxels",
                  "narrow_band");
    }

    // --- brick ---
    if (j.contains("brick") && j["brick"].is_object() && j["brick"].contains("size")) {
        m.brick_size = j["brick"]["size"].get<int>();
        if (m.brick_size != 32 && m.brick_size != 64 && m.brick_size != 128) {
            add_error(result, E1006, "brick.size must be 32, 64, or 128, got: " +
                      std::to_string(m.brick_size), "brick.size");
        }
    } else {
        add_error(result, E1001, "Missing required field: brick.size", "brick");
    }

    // --- dtype ---
    if (j.contains("dtype") && j["dtype"].is_string()) {
        m.dtype = j["dtype"].get<std::string>();
        if (m.dtype != "f16" && m.dtype != "f32") {
            add_error(result, E1002, "dtype must be \"f16\" or \"f32\", got: " + m.dtype, "dtype");
        }
    } else {
        add_error(result, E1001, "Missing or invalid dtype", "dtype");
    }

    // --- background_value_mm ---
    if (j.contains("background_value_mm") && j["background_value_mm"].is_number()) {
        m.background_value_mm = j["background_value_mm"].get<float>();
        if (m.background_value_mm <= 0) {
            add_error(result, E1007, "background_value_mm must be > 0", "background_value_mm");
        }
    } else {
        add_error(result, E1001, "Missing or invalid background_value_mm", "background_value_mm");
    }

    // --- hashes (field required, values optional) ---
    if (!j.contains("hashes") || !j["hashes"].is_object()) {
        add_error(result, E1001, "Missing required field: hashes", "hashes");
    } else {
        auto& h = j["hashes"];
        if (h.contains("manifest_sha256") && h["manifest_sha256"].is_string())
            m.manifest_sha256 = h["manifest_sha256"].get<std::string>();
        if (h.contains("bricks_bin_sha256") && h["bricks_bin_sha256"].is_string())
            m.bricks_bin_sha256 = h["bricks_bin_sha256"].get<std::string>();
        if (h.contains("bricks_index_sha256") && h["bricks_index_sha256"].is_string())
            m.bricks_index_sha256 = h["bricks_index_sha256"].get<std::string>();
    }

    // ========== Cross-field consistency rules (§4.3) ==========

    constexpr float eps_mm = 1e-6f;

    // aabb_size[i] == dims[i] * voxel_size
    if (m.voxel_size > 0 && m.dims[0] > 0) {
        for (int i = 0; i < 3; ++i) {
            float expected = m.dims[i] * m.voxel_size;
            if (std::abs(m.aabb_size[i] - expected) > eps_mm) {
                add_error(result, E1002,
                    "aabb_size[" + std::to_string(i) + "]=" + std::to_string(m.aabb_size[i]) +
                    " != dims[" + std::to_string(i) + "]*voxel_size=" + std::to_string(expected),
                    "aabb_size");
            }
        }
    }

    // background_value_mm >= half_width_voxels * voxel_size
    if (m.background_value_mm > 0 && m.half_width_voxels >= 1 && m.voxel_size > 0) {
        float band_world = m.half_width_voxels * m.voxel_size;
        if (m.background_value_mm < band_world) {
            add_error(result, E1007,
                "background_value_mm (" + std::to_string(m.background_value_mm) +
                ") must be >= narrow_band.half_width_voxels * voxel_size (" +
                std::to_string(band_world) + ")",
                "background_value_mm");
        }
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
