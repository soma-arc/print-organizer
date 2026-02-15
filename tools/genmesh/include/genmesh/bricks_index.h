#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "genmesh/exit_code.h"
#include "genmesh/manifest.h"

namespace genmesh {

/// Single brick entry from bricks.index.json
struct BrickEntry {
    int bx = 0;
    int by = 0;
    int bz = 0;
    int64_t offset_bytes = 0;
    int64_t payload_bytes = 0;
    std::string encoding;            // v1: "raw" only
    std::optional<std::string> crc32; // optional hex string
};

/// Parsed bricks index
struct BricksIndex {
    int version = 0;
    int brick_size = 0;
    std::string dtype;
    std::string axis_order;
    std::array<int, 3> dims = {};
    std::vector<BrickEntry> bricks;
};

/// Result of bricks index loading
struct BricksIndexResult {
    BricksIndex index;
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::vector<ValidationError> errors;  // reuse ValidationError from manifest.h
};

/// Load and validate bricks.index.json from a file path.
/// Validates internal consistency and cross-checks against manifest.
BricksIndexResult load_bricks_index(const std::string& path,
                                    const Manifest& manifest);

}  // namespace genmesh
