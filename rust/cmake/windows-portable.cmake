# Distribution builds must not inherit the CI runner's SIMD capabilities.
# This whisper.cpp version does not runtime-dispatch these compiler flags.
foreach(feature NATIVE AVX AVX2 AVX512 AVX512_VBMI AVX512_VNNI AVX512_BF16 FMA F16C AMX_TILE AMX_INT8 AMX_BF16)
    set(GGML_${feature} OFF CACHE BOOL "Portable Windows x64 baseline" FORCE)
endforeach()
