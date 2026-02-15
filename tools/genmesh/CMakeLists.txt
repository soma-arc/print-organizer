cmake_minimum_required(VERSION 3.20)
project(genmesh LANGUAGES CXX)

cmake_policy(SET CMP0167 NEW)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(OpenVDB CONFIG REQUIRED)
find_package(nlohmann_json CONFIG REQUIRED)

# src/以下のソースファイルを収集
file(GLOB_RECURSE SOURCES "src/*.cpp")

add_executable(genmesh ${SOURCES})
target_link_libraries(genmesh PRIVATE 
    OpenVDB::openvdb
    nlohmann_json::nlohmann_json
)