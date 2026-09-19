# Only launched after runtime AVX2 + FMA + F16C checks. Never inherit CI AVX512.
include("${CMAKE_CURRENT_LIST_DIR}/windows-portable.cmake")
foreach(feature AVX AVX2 FMA F16C)
    set(GGML_${feature} ON CACHE BOOL "Runtime-gated Windows AVX2 worker" FORCE)
endforeach()
