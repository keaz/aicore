#include <stdint.h>

typedef struct {
    const char *ptr;
    int64_t len;
    int64_t cap;
} AicoreString;

AicoreString ffi_string_repeat(const char *seed_ptr, int64_t seed_len, int64_t seed_cap, int64_t times) {
    static char buffer[256];
    (void)seed_cap;
    if (seed_ptr == 0 || seed_len <= 0 || times <= 0) {
        AicoreString empty = {
            .ptr = buffer,
            .len = 0,
            .cap = 0,
        };
        return empty;
    }

    int64_t out_len = 0;
    for (int64_t i = 0; i < times; i++) {
        for (int64_t j = 0; j < seed_len && out_len < 255; j++) {
            buffer[out_len++] = seed_ptr[j];
        }
    }
    buffer[out_len] = '\0';

    AicoreString value = {
        .ptr = buffer,
        .len = out_len,
        .cap = out_len,
    };
    return value;
}
