// T1.3 Output directory handling tests
#include <cassert>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>

#include "genmesh/output.h"
#include "genmesh/error_code.h"
#include "genmesh/log.h"

namespace fs = std::filesystem;

// Unique temp directory per test run
static std::string test_root() {
    return "_test_output_tmp";
}

// Clean up helper
static void cleanup() {
    std::error_code ec;
    fs::remove_all(test_root(), ec);
}

void test_creates_dir() {
    cleanup();
    std::string dir = test_root() + "/a/b/c";
    auto r = genmesh::prepare_output_dir(dir, true, false, false);
    assert(r.ok);
    assert(fs::is_directory(dir));
    cleanup();
    std::cout << "  PASS: test_creates_dir\n";
}

void test_existing_dir_no_files() {
    cleanup();
    std::string dir = test_root() + "/empty";
    fs::create_directories(dir);
    auto r = genmesh::prepare_output_dir(dir, true, false, false);
    assert(r.ok);
    cleanup();
    std::cout << "  PASS: test_existing_dir_no_files\n";
}

void test_existing_stl_no_force() {
    cleanup();
    std::string dir = test_root() + "/has_stl";
    fs::create_directories(dir);
    std::ofstream(dir + "/mesh.stl") << "dummy";
    auto r = genmesh::prepare_output_dir(dir, true, false, false);
    assert(!r.ok);
    assert(r.exit_code == genmesh::ExitCode::IoError);
    assert(r.error_code == std::string(genmesh::E2005));
    cleanup();
    std::cout << "  PASS: test_existing_stl_no_force\n";
}

void test_existing_stl_with_force() {
    cleanup();
    std::string dir = test_root() + "/has_stl_force";
    fs::create_directories(dir);
    std::ofstream(dir + "/mesh.stl") << "dummy";
    auto r = genmesh::prepare_output_dir(dir, true, false, true);
    assert(r.ok);
    cleanup();
    std::cout << "  PASS: test_existing_stl_with_force\n";
}

void test_existing_report_no_force() {
    cleanup();
    std::string dir = test_root() + "/has_report";
    fs::create_directories(dir);
    std::ofstream(dir + "/report.json") << "{}";
    auto r = genmesh::prepare_output_dir(dir, true, false, false);
    assert(!r.ok);
    assert(r.error_code == std::string(genmesh::E2005));
    cleanup();
    std::cout << "  PASS: test_existing_report_no_force\n";
}

void test_existing_vdb_no_force() {
    cleanup();
    std::string dir = test_root() + "/has_vdb";
    fs::create_directories(dir);
    std::ofstream(dir + "/volume.vdb") << "dummy";
    // write_vdb = true → should detect it
    auto r = genmesh::prepare_output_dir(dir, false, true, false);
    assert(!r.ok);
    assert(r.error_code == std::string(genmesh::E2005));
    cleanup();
    std::cout << "  PASS: test_existing_vdb_no_force\n";
}

void test_vdb_exists_but_write_vdb_false() {
    cleanup();
    std::string dir = test_root() + "/has_vdb_ignore";
    fs::create_directories(dir);
    std::ofstream(dir + "/volume.vdb") << "dummy";
    // write_vdb = false → should NOT detect volume.vdb
    auto r = genmesh::prepare_output_dir(dir, false, false, false);
    assert(r.ok);
    cleanup();
    std::cout << "  PASS: test_vdb_exists_but_write_vdb_false\n";
}

void test_existing_report_with_force() {
    cleanup();
    std::string dir = test_root() + "/has_report_force";
    fs::create_directories(dir);
    std::ofstream(dir + "/report.json") << "{}";
    auto r = genmesh::prepare_output_dir(dir, true, false, true);
    assert(r.ok);
    cleanup();
    std::cout << "  PASS: test_existing_report_with_force\n";
}

int main() {
    // suppress log noise during tests
    genmesh::min_log_level() = genmesh::LogLevel::Error;

    std::cout << "=== T1.3 Output directory tests ===\n";

    test_creates_dir();
    test_existing_dir_no_files();
    test_existing_stl_no_force();
    test_existing_stl_with_force();
    test_existing_report_no_force();
    test_existing_vdb_no_force();
    test_vdb_exists_but_write_vdb_false();
    test_existing_report_with_force();

    // final cleanup
    cleanup();

    std::cout << "=== All T1.3 tests passed ===\n";
    return 0;
}
