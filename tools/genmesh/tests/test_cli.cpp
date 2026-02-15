// T1.1 CLI argument parsing tests
#include <cassert>
#include <cstring>
#include <iostream>
#include <string>
#include <vector>

#include "genmesh/cli.h"
#include "genmesh/exit_code.h"

// Helper: build argv from initializer list of C strings
struct ArgBuilder {
    std::vector<std::string> storage;
    std::vector<char*> ptrs;

    ArgBuilder(std::initializer_list<const char*> args) {
        for (auto a : args) storage.emplace_back(a);
        for (auto& s : storage) ptrs.push_back(s.data());
    }
    int argc() const { return static_cast<int>(ptrs.size()); }
    char** argv() { return ptrs.data(); }
};

void test_full_args() {
    ArgBuilder ab{"genmesh", "--manifest", "p.json", "--in", "data/", "--out", "out/"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(r.ok);
    assert(r.args.manifest_path == "p.json");
    assert(r.args.in_dir == "data/");
    assert(r.args.out_dir == "out/");
    assert(r.args.write_stl == true);
    assert(r.args.write_vdb == false);
    assert(r.args.force == false);
    assert(!r.args.iso.has_value());
    assert(!r.args.adaptivity.has_value());
    std::cout << "  PASS: test_full_args\n";
}

void test_missing_required() {
    // missing --out
    ArgBuilder ab{"genmesh", "--manifest", "p.json", "--in", "data/"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(!r.ok);
    assert(r.exit_code == static_cast<int>(genmesh::ExitCode::ValidationFailure));
    assert(r.error_msg.find("--out") != std::string::npos);
    std::cout << "  PASS: test_missing_required\n";
}

void test_unknown_arg() {
    ArgBuilder ab{"genmesh", "--manifest", "p.json", "--in", "d/", "--out", "o/", "--bogus"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(!r.ok);
    assert(r.exit_code == static_cast<int>(genmesh::ExitCode::General));
    assert(r.error_msg.find("Unknown") != std::string::npos);
    std::cout << "  PASS: test_unknown_arg\n";
}

void test_help() {
    ArgBuilder ab{"genmesh", "--help"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(r.ok);
    assert(r.args.help);
    std::cout << "  PASS: test_help\n";
}

void test_no_args_shows_help() {
    ArgBuilder ab{"genmesh"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(r.ok);
    assert(r.args.help);
    std::cout << "  PASS: test_no_args_shows_help\n";
}

void test_optional_flags() {
    ArgBuilder ab{"genmesh", "--manifest", "p.json", "--in", "d/", "--out", "o/",
                  "--no-write-stl", "--write-vdb", "--force",
                  "--iso", "0.5", "--adaptivity", "0.3",
                  "--log-level", "debug"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(r.ok);
    assert(r.args.write_stl == false);
    assert(r.args.write_vdb == true);
    assert(r.args.force == true);
    assert(r.args.iso.has_value());
    assert(std::abs(r.args.iso.value() - 0.5f) < 1e-6f);
    assert(r.args.adaptivity.has_value());
    assert(std::abs(r.args.adaptivity.value() - 0.3f) < 1e-6f);
    assert(r.args.log_level == "debug");
    std::cout << "  PASS: test_optional_flags\n";
}

void test_debug_generate() {
    // debug-generate only needs --out
    ArgBuilder ab{"genmesh", "--debug-generate", "sphere", "--out", "o/"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(r.ok);
    assert(r.args.debug_generate == "sphere");
    assert(r.args.out_dir == "o/");
    std::cout << "  PASS: test_debug_generate\n";
}

void test_debug_generate_missing_out() {
    ArgBuilder ab{"genmesh", "--debug-generate", "sphere"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(!r.ok);
    assert(r.exit_code == static_cast<int>(genmesh::ExitCode::ValidationFailure));
    std::cout << "  PASS: test_debug_generate_missing_out\n";
}

void test_invalid_log_level() {
    ArgBuilder ab{"genmesh", "--manifest", "p.json", "--in", "d/", "--out", "o/",
                  "--log-level", "verbose"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(!r.ok);
    assert(r.exit_code == static_cast<int>(genmesh::ExitCode::General));
    std::cout << "  PASS: test_invalid_log_level\n";
}

void test_missing_value() {
    ArgBuilder ab{"genmesh", "--manifest"};
    auto r = genmesh::parse_args(ab.argc(), ab.argv());
    assert(!r.ok);
    assert(r.exit_code == static_cast<int>(genmesh::ExitCode::General));
    std::cout << "  PASS: test_missing_value\n";
}

int main() {
    std::cout << "=== T1.1 CLI parsing tests ===\n";

    test_full_args();
    test_missing_required();
    test_unknown_arg();
    test_help();
    test_no_args_shows_help();
    test_optional_flags();
    test_debug_generate();
    test_debug_generate_missing_out();
    test_invalid_log_level();
    test_missing_value();

    std::cout << "=== All T1.1 tests passed ===\n";
    return 0;
}
