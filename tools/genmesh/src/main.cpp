#include <cstdlib>
#include <iostream>

#include "genmesh/cli.h"
#include "genmesh/exit_code.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

int main(int argc, char* argv[]) {
    auto parsed = genmesh::parse_args(argc, argv);

    if (parsed.args.help) {
        genmesh::print_usage();
        return static_cast<int>(genmesh::ExitCode::Success);
    }

    if (!parsed.ok) {
        genmesh::log_error(genmesh::E9001, parsed.error_msg);
        return parsed.exit_code;
    }

    // Set global log level
    genmesh::min_log_level() = genmesh::parse_log_level(parsed.args.log_level);

    genmesh::log_info("GENMESH_I0000", "genmesh v0.1.0 starting", {
        {"manifest", parsed.args.manifest_path},
        {"in", parsed.args.in_dir},
        {"out", parsed.args.out_dir},
    });

    // TODO: Phase 1.2+ processing
    return static_cast<int>(genmesh::ExitCode::Success);
}
