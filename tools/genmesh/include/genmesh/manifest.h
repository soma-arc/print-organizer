#pragma once

#include <array>
#include <optional>
#include <string>
#include <vector>

#include "genmesh/exit_code.h"

namespace genmesh {

/// Parsed manifest (project.json) per spec section 4
struct Manifest {
    int version = 0;

    // coordinate_system (v1: fixed values)
    std::string handedness;
    std::string up_axis;
    std::string front_axis;

    std::string units;

    std::array<float, 3> aabb_min = {};
    std::array<float, 3> aabb_size = {};
    float voxel_size = 0.0f;
    std::array<int, 3> dims = {};

    std::string sample_at;
    std::string axis_order;
    std::string distance_sign;

    float iso = 0.0f;
    float adaptivity = 0.0f;

    // narrow_band
    int half_width_voxels = 0;

    // brick
    int brick_size = 64;

    std::string dtype;
    float background_value_mm = 1000.0f;

    // hashes (optional values)
    std::optional<std::string> manifest_sha256;
    std::optional<std::string> bricks_bin_sha256;
    std::optional<std::string> bricks_index_sha256;
};

/// Validation error entry
struct ValidationError {
    std::string code;     // GENMESH_E* code
    std::string message;
    std::string field;    // field name for context
};

/// Result of manifest loading
struct ManifestResult {
    Manifest manifest;
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::vector<ValidationError> errors;
};

/// Load and validate manifest from a JSON file.
/// Returns ManifestResult with all validation errors collected (not just first).
ManifestResult load_manifest(const std::string& path);

}  // namespace genmesh
