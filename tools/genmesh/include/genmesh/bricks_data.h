#pragma once

#include <cstdint>
#include <string>
#include <vector>

#include "genmesh/bricks_index.h"
#include "genmesh/exit_code.h"
#include "genmesh/manifest.h"

namespace genmesh {

/// A loaded brick: dense B^3 float values in x-fastest order.
struct BrickData {
    int bx = 0;
    int by = 0;
    int bz = 0;
    std::vector<float> values;  // B^3 floats (x-fastest order)
};

/// Result of loading bricks.bin
struct BricksDataResult {
    std::vector<BrickData> bricks;
    bool ok = false;
    ExitCode exit_code = ExitCode::Success;
    std::vector<ValidationError> errors;
};

/// Load brick data from bricks.bin using the parsed index and manifest.
///
/// - Validates that each brick's (offset_bytes + payload_bytes) is within file size.
/// - Reads raw f32 or f16 data, converting f16 → float.
/// - If crc32 is present in a BrickEntry, verifies CRC32 of the raw payload.
BricksDataResult load_bricks_bin(const std::string& bin_path,
                                 const BricksIndex& index,
                                 const Manifest& manifest);

}  // namespace genmesh
