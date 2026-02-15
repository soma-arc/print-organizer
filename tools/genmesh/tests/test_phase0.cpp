// Minimal test runner (no external framework for Phase 0)
#include <cassert>
#include <iostream>
#include <sstream>
#include <string>

#include "genmesh/exit_code.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

// Helper: capture stderr output from a callable
template <typename F>
std::string capture_stderr(F&& fn) {
    // redirect std::cerr to a stringstream
    std::ostringstream captured;
    auto* old_buf = std::cerr.rdbuf(captured.rdbuf());
    fn();
    std::cerr.rdbuf(old_buf);
    return captured.str();
}

void test_exit_codes() {
    assert(static_cast<int>(genmesh::ExitCode::Success)           == 0);
    assert(static_cast<int>(genmesh::ExitCode::General)           == 1);
    assert(static_cast<int>(genmesh::ExitCode::ValidationFailure) == 2);
    assert(static_cast<int>(genmesh::ExitCode::IoError)           == 3);
    assert(static_cast<int>(genmesh::ExitCode::EnvironmentError)  == 4);
    assert(static_cast<int>(genmesh::ExitCode::ProcessingError)   == 5);
    std::cout << "  PASS: test_exit_codes\n";
}

void test_error_codes() {
    // Spot-check representative codes
    assert(genmesh::E1001 == "GENMESH_E1001");
    assert(genmesh::E2001 == "GENMESH_E2001");
    assert(genmesh::E3001 == "GENMESH_E3001");
    assert(genmesh::E5001 == "GENMESH_E5001");
    assert(genmesh::E9001 == "GENMESH_E9001");
    assert(genmesh::W5001 == "GENMESH_W5001");
    std::cout << "  PASS: test_error_codes\n";
}

void test_log_level_parse() {
    assert(genmesh::parse_log_level("error") == genmesh::LogLevel::Error);
    assert(genmesh::parse_log_level("warn")  == genmesh::LogLevel::Warn);
    assert(genmesh::parse_log_level("info")  == genmesh::LogLevel::Info);
    assert(genmesh::parse_log_level("debug") == genmesh::LogLevel::Debug);
    assert(genmesh::parse_log_level("bogus") == genmesh::LogLevel::Info);
    std::cout << "  PASS: test_log_level_parse\n";
}

void test_log_output_format() {
    genmesh::min_log_level() = genmesh::LogLevel::Debug;

    // Basic error log
    auto out1 = capture_stderr([] {
        genmesh::log_error(genmesh::E1001, "manifest missing field",
                           {{"field", "dims"}, {"path", "project.json"}});
    });
    assert(out1.find("ERROR GENMESH_E1001: manifest missing field") != std::string::npos);
    assert(out1.find("| field=dims path=project.json") != std::string::npos);

    // Log without context
    auto out2 = capture_stderr([] {
        genmesh::log_info("GENMESH_E0000", "simple message");
    });
    assert(out2.find("INFO GENMESH_E0000: simple message") != std::string::npos);
    assert(out2.find("|") == std::string::npos);

    std::cout << "  PASS: test_log_output_format\n";
}

void test_log_level_filter() {
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    // Info should be filtered out
    auto out = capture_stderr([] {
        genmesh::log_info("GENMESH_E0000", "should not appear");
    });
    assert(out.empty());

    // Error should pass through
    auto out2 = capture_stderr([] {
        genmesh::log_error("GENMESH_E0000", "should appear");
    });
    assert(!out2.empty());

    // Reset to default
    genmesh::min_log_level() = genmesh::LogLevel::Info;

    std::cout << "  PASS: test_log_level_filter\n";
}

int main() {
    std::cout << "=== genmesh Phase 0 tests ===\n";

    test_exit_codes();
    test_error_codes();
    test_log_level_parse();
    test_log_output_format();
    test_log_level_filter();

    std::cout << "=== All tests passed ===\n";
    return 0;
}
