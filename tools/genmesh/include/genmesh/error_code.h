#pragma once

#include <string_view>

namespace genmesh {

/// Error / warning code constants per spec section 9.3-9.4.
///
/// Format: GENMESH_[E|W]<4 digits>
///   E1xxx  input / validation (manifest, index, bin)
///   E2xxx  I/O (read, write, path, permission)
///   E3xxx  environment / dependency (OpenVDB init, DLL, version)
///   E4xxx  VDB build (Transform, grid, background)
///   E5xxx  meshing (volumeToMesh, output mesh)
///   E9xxx  unexpected (exception, bug)

// --- E1xxx: input / validation -------------------------------------------
inline constexpr std::string_view E1001 = "GENMESH_E1001";  // manifest required field missing
inline constexpr std::string_view E1002 = "GENMESH_E1002";  // manifest consistency violation
inline constexpr std::string_view E1003 = "GENMESH_E1003";  // manifest coordinate_system mismatch
inline constexpr std::string_view E1004 = "GENMESH_E1004";  // manifest distance_sign mismatch
inline constexpr std::string_view E1005 = "GENMESH_E1005";  // manifest adaptivity out of range
inline constexpr std::string_view E1006 = "GENMESH_E1006";  // manifest brick.size invalid
inline constexpr std::string_view E1007 = "GENMESH_E1007";  // manifest background_value_mm invalid
inline constexpr std::string_view E1101 = "GENMESH_E1101";  // bricks.index.json inconsistency
inline constexpr std::string_view E1102 = "GENMESH_E1102";  // bricks.index.json duplicate brick
inline constexpr std::string_view E1103 = "GENMESH_E1103";  // bricks.index.json brick out of range
inline constexpr std::string_view E1104 = "GENMESH_E1104";  // bricks payload_bytes mismatch
inline constexpr std::string_view E1105 = "GENMESH_E1105";  // bricks offset out of file range
inline constexpr std::string_view E1106 = "GENMESH_E1106";  // bricks CRC32 mismatch

// --- E2xxx: I/O ----------------------------------------------------------
inline constexpr std::string_view E2001 = "GENMESH_E2001";  // bricks.bin read failure
inline constexpr std::string_view E2002 = "GENMESH_E2002";  // manifest read failure
inline constexpr std::string_view E2003 = "GENMESH_E2003";  // bricks.index.json read failure
inline constexpr std::string_view E2004 = "GENMESH_E2004";  // output dir creation failure
inline constexpr std::string_view E2005 = "GENMESH_E2005";  // output file already exists
inline constexpr std::string_view E2101 = "GENMESH_E2101";  // report.json write failure
inline constexpr std::string_view E2102 = "GENMESH_E2102";  // STL write failure
inline constexpr std::string_view E2103 = "GENMESH_E2103";  // VDB write failure

// --- E3xxx: environment / dependency -------------------------------------
inline constexpr std::string_view E3001 = "GENMESH_E3001";  // openvdb::initialize failure

// --- E4xxx: VDB build ----------------------------------------------------
inline constexpr std::string_view E4001 = "GENMESH_E4001";  // VDB grid creation failure
inline constexpr std::string_view E4002 = "GENMESH_E4002";  // VDB voxel insertion failure

// --- E5xxx: meshing ------------------------------------------------------
inline constexpr std::string_view E5001 = "GENMESH_E5001";  // volumeToMesh failure
inline constexpr std::string_view E5002 = "GENMESH_E5002";  // empty mesh (zero triangles)

// --- E9xxx: unexpected ---------------------------------------------------
inline constexpr std::string_view E9001 = "GENMESH_E9001";  // unhandled exception

// --- Warnings (W) --------------------------------------------------------
inline constexpr std::string_view W1001 = "GENMESH_W1001";  // optional field missing
inline constexpr std::string_view W5001 = "GENMESH_W5001";  // degenerate triangles detected
inline constexpr std::string_view W5002 = "GENMESH_W5002";  // winding inversion suspected

}  // namespace genmesh
