cmake_minimum_required(VERSION 3.20)
project(genmesh LANGUAGES CXX)

cmake_policy(SET CMP0167 NEW)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(OpenVDB CONFIG REQUIRED)
find_package(nlohmann_json CONFIG REQUIRED)

# ---------- main executable ----------
file(GLOB_RECURSE SOURCES "src/*.cpp")

add_executable(genmesh ${SOURCES})
target_include_directories(genmesh PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(genmesh PRIVATE
    OpenVDB::openvdb
    nlohmann_json::nlohmann_json
)

# ---------- tests ----------
enable_testing()

file(GLOB_RECURSE TEST_SOURCES "tests/*.cpp")
if(TEST_SOURCES)
    add_executable(genmesh_tests ${TEST_SOURCES})
    target_include_directories(genmesh_tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/include)
    target_link_libraries(genmesh_tests PRIVATE
        nlohmann_json::nlohmann_json
    )
    add_test(NAME genmesh_tests COMMAND genmesh_tests)
endif()