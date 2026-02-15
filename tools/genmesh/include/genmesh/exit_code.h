#pragma once

namespace genmesh {

/// Exit codes per spec section 9.1
enum class ExitCode : int {
    Success             = 0,
    General             = 1,  // uncategorized / bug
    ValidationFailure   = 2,  // manifest / bricks inconsistency
    IoError             = 3,  // file open / read / write / mkdir
    EnvironmentError    = 4,  // OpenVDB init, missing DLL, etc.
    ProcessingError     = 5,  // VDB build / meshing failure
};

}  // namespace genmesh
