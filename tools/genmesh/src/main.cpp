#include <cstdlib>
#include <iostream>

#include "genmesh/exit_code.h"
#include "genmesh/error_code.h"

int main(int argc, char* argv[]) {
    // placeholder – replaced in T1.1
    std::cerr << "genmesh v0.1.0" << std::endl;
    return static_cast<int>(genmesh::ExitCode::Success);
}
