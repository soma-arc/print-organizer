cmake_minimum_required(VERSION 3.20)
project(genmesh LANGUAGES CXX)

cmake_policy(SET CMP0167 NEW)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# MSVC: /bigobj needed for OpenVDB heavy template instantiations (VolumeToMesh)
if(MSVC)
    add_compile_options(/bigobj)
endif()

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

# ---------- library (for tests to link against) ----------
file(GLOB_RECURSE LIB_SOURCES "src/*.cpp")
list(FILTER LIB_SOURCES EXCLUDE REGEX "main\\.cpp$")

add_library(genmesh_lib STATIC ${LIB_SOURCES})
target_include_directories(genmesh_lib PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_link_libraries(genmesh_lib PUBLIC
    OpenVDB::openvdb
    nlohmann_json::nlohmann_json
)

# ---------- tests ----------
enable_testing()

file(GLOB TEST_FILES "tests/test_*.cpp")
foreach(TEST_FILE ${TEST_FILES})
    get_filename_component(TEST_NAME ${TEST_FILE} NAME_WE)
    add_executable(${TEST_NAME} ${TEST_FILE})
    target_link_libraries(${TEST_NAME} PRIVATE genmesh_lib)
    add_test(NAME ${TEST_NAME} COMMAND ${TEST_NAME})
endforeach()