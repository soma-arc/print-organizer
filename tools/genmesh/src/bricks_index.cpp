#include "genmesh/bricks_index.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

#include <nlohmann/json.hpp>
#include <cmath>
#include <fstream>
#include <set>
#include <string>
#include <tuple>

namespace genmesh {

using json = nlohmann::json;

// ---------- helpers ----------

static void add_error(BricksIndexResult& r, std::string_view code,
                      const std::string& msg, const std::string& field = "") {
    r.errors.push_back({std::string(code), msg, field});
    log_error(code, msg, field.empty() ? std::vector<KV>{} : std::vector<KV>{{"field", field}});
}

// ---------- load + validate ----------

BricksIndexResult load_bricks_index(const std::string& path,
                                    const Manifest& manifest) {
    BricksIndexResult result;

    // --- read file ---
    std::ifstream ifs(path);
    if (!ifs.is_open()) {
        add_error(result, E2003, "Cannot open bricks index: " + path);
        result.exit_code = ExitCode::IoError;
        return result;
    }

    json j;
    try {
        j = json::parse(ifs);
    } catch (const json::parse_error& e) {
        add_error(result, E2003, std::string("bricks.index.json parse error: ") + e.what());
        result.exit_code = ExitCode::ValidationFailure;
        return result;
    }

    auto& idx = result.index;

    // --- version ---
    if (j.contains("version") && j["version"].is_number_integer()) {
        idx.version = j["version"].get<int>();
        if (idx.version != 1) {
            add_error(result, E1101,
                      "Unsupported bricks index version: " + std::to_string(idx.version),
                      "version");
        }
    } else {
        add_error(result, E1101, "Missing or invalid field: version", "version");
    }

    // --- brick_size ---
    if (j.contains("brick_size") && j["brick_size"].is_number_integer()) {
        idx.brick_size = j["brick_size"].get<int>();
    } else {
        add_error(result, E1101, "Missing or invalid field: brick_size", "brick_size");
    }

    // --- dtype ---
    if (j.contains("dtype") && j["dtype"].is_string()) {
        idx.dtype = j["dtype"].get<std::string>();
    } else {
        add_error(result, E1101, "Missing or invalid field: dtype", "dtype");
    }

    // --- axis_order ---
    if (j.contains("axis_order") && j["axis_order"].is_string()) {
        idx.axis_order = j["axis_order"].get<std::string>();
        if (idx.axis_order != "x-fastest") {
            add_error(result, E1101,
                      "axis_order must be \"x-fastest\", got: " + idx.axis_order,
                      "axis_order");
        }
    } else {
        add_error(result, E1101, "Missing or invalid field: axis_order", "axis_order");
    }

    // --- dims ---
    if (j.contains("dims") && j["dims"].is_array() && j["dims"].size() == 3) {
        for (int i = 0; i < 3; ++i) {
            idx.dims[i] = j["dims"][i].get<int>();
        }
    } else {
        add_error(result, E1101, "Missing or invalid field: dims (expected int[3])", "dims");
    }

    // ========== Cross-check with manifest (§5.6) ==========

    if (idx.brick_size != 0 && idx.brick_size != manifest.brick_size) {
        add_error(result, E1101,
                  "brick_size mismatch: index=" + std::to_string(idx.brick_size) +
                  " manifest=" + std::to_string(manifest.brick_size),
                  "brick_size");
    }

    if (!idx.dtype.empty() && idx.dtype != manifest.dtype) {
        add_error(result, E1101,
                  "dtype mismatch: index=\"" + idx.dtype +
                  "\" manifest=\"" + manifest.dtype + "\"",
                  "dtype");
    }

    if (!idx.axis_order.empty() && idx.axis_order != manifest.axis_order) {
        add_error(result, E1101,
                  "axis_order mismatch: index=\"" + idx.axis_order +
                  "\" manifest=\"" + manifest.axis_order + "\"",
                  "axis_order");
    }

    if (idx.dims[0] != 0) {
        for (int i = 0; i < 3; ++i) {
            if (idx.dims[i] != manifest.dims[i]) {
                add_error(result, E1101,
                          "dims[" + std::to_string(i) + "] mismatch: index=" +
                          std::to_string(idx.dims[i]) + " manifest=" +
                          std::to_string(manifest.dims[i]),
                          "dims");
            }
        }
    }

    // ========== Parse bricks array ==========

    const int B = idx.brick_size > 0 ? idx.brick_size : manifest.brick_size;
    const int sizeof_dtype = (idx.dtype == "f16" || manifest.dtype == "f16") ? 2 : 4;
    const int64_t expected_payload = static_cast<int64_t>(B) * B * B * sizeof_dtype;

    // Compute max brick coordinates from dims
    auto max_bcoord = [&](int axis) -> int {
        int dim = (idx.dims[axis] > 0) ? idx.dims[axis] : manifest.dims[axis];
        return (dim > 0 && B > 0) ? static_cast<int>(std::ceil(static_cast<double>(dim) / B)) - 1 : 0;
    };
    const int max_bx = max_bcoord(0);
    const int max_by = max_bcoord(1);
    const int max_bz = max_bcoord(2);

    if (j.contains("bricks") && j["bricks"].is_array()) {
        // Duplicate detection
        std::set<std::tuple<int, int, int>> seen;

        for (size_t bi = 0; bi < j["bricks"].size(); ++bi) {
            const auto& bj = j["bricks"][bi];
            BrickEntry entry;

            std::string prefix = "bricks[" + std::to_string(bi) + "]";

            if (bj.contains("bx") && bj["bx"].is_number_integer()) {
                entry.bx = bj["bx"].get<int>();
            } else {
                add_error(result, E1101, prefix + ".bx missing or invalid", "bricks");
            }

            if (bj.contains("by") && bj["by"].is_number_integer()) {
                entry.by = bj["by"].get<int>();
            } else {
                add_error(result, E1101, prefix + ".by missing or invalid", "bricks");
            }

            if (bj.contains("bz") && bj["bz"].is_number_integer()) {
                entry.bz = bj["bz"].get<int>();
            } else {
                add_error(result, E1101, prefix + ".bz missing or invalid", "bricks");
            }

            if (bj.contains("offset_bytes") && bj["offset_bytes"].is_number_integer()) {
                entry.offset_bytes = bj["offset_bytes"].get<int64_t>();
            } else {
                add_error(result, E1101, prefix + ".offset_bytes missing or invalid", "bricks");
            }

            if (bj.contains("payload_bytes") && bj["payload_bytes"].is_number_integer()) {
                entry.payload_bytes = bj["payload_bytes"].get<int64_t>();
            } else {
                add_error(result, E1101, prefix + ".payload_bytes missing or invalid", "bricks");
            }

            if (bj.contains("encoding") && bj["encoding"].is_string()) {
                entry.encoding = bj["encoding"].get<std::string>();
                if (entry.encoding != "raw") {
                    add_error(result, E1101,
                              prefix + ".encoding must be \"raw\", got: " + entry.encoding,
                              "bricks");
                }
            } else {
                add_error(result, E1101, prefix + ".encoding missing or invalid", "bricks");
            }

            if (bj.contains("crc32") && bj["crc32"].is_string()) {
                entry.crc32 = bj["crc32"].get<std::string>();
            }

            // --- payload_bytes check (§5.6) ---
            if (entry.encoding == "raw" && entry.payload_bytes != expected_payload) {
                add_error(result, E1104,
                          prefix + ".payload_bytes=" + std::to_string(entry.payload_bytes) +
                          " != B^3*sizeof(dtype)=" + std::to_string(expected_payload),
                          "bricks");
            }

            // --- brick coordinate range check (§5.4) ---
            if (entry.bx < 0 || entry.bx > max_bx ||
                entry.by < 0 || entry.by > max_by ||
                entry.bz < 0 || entry.bz > max_bz) {
                add_error(result, E1103,
                          prefix + " brick (" + std::to_string(entry.bx) + "," +
                          std::to_string(entry.by) + "," + std::to_string(entry.bz) +
                          ") out of range [0," + std::to_string(max_bx) + "]x[0," +
                          std::to_string(max_by) + "]x[0," + std::to_string(max_bz) + "]",
                          "bricks");
            }

            // --- duplicate check (§5.6) ---
            auto key = std::make_tuple(entry.bx, entry.by, entry.bz);
            if (!seen.insert(key).second) {
                add_error(result, E1102,
                          prefix + " duplicate brick (" + std::to_string(entry.bx) + "," +
                          std::to_string(entry.by) + "," + std::to_string(entry.bz) + ")",
                          "bricks");
            }

            idx.bricks.push_back(std::move(entry));
        }
    } else {
        add_error(result, E1101, "Missing or invalid field: bricks", "bricks");
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
